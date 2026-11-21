//! Narrative snapshot extension and a project-local restore intent. No media copies.
use super::*;
use crate::narrative::{self, Production};
const PENDING: &str = "edit-restore.pending.json";

pub(crate) fn ensure_ready(data: &FsPath, id: Uuid) -> Result<(), ApiError> {
    if store::project_dir(data, id).join(PENDING).exists() {
        return Err(ApiError(StatusCode::SERVICE_UNAVAILABLE,"Ripristino da completare: libera eventuale spazio e riavvia la suite. Dati e backup sono conservati; nessun altro salvataggio viene applicato al progetto.".into()));
    }
    Ok(())
}
pub(super) fn expected(s: &AppState, p: &Project, r: &Request) -> Result<(), ApiError> {
    if let Some(revision) = r.expected_narrative_revision {
        if narrative::load(s, p)?.revision != revision {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "Piano narrativo cambiato: aggiorna il confronto prima di continuare".into(),
            ));
        }
    }
    Ok(())
}
fn referenced(value: &Value, id: Uuid) -> bool {
    match value {
        Value::String(s) => s == &id.to_string(),
        Value::Array(a) => a.iter().any(|v| referenced(v, id)),
        Value::Object(o) => o.values().any(|v| referenced(v, id)),
        _ => false,
    }
}
impl Version {
    pub(super) fn capture_all(
        s: &AppState,
        p: &Project,
        label: String,
        recovery: bool,
    ) -> Result<Self, ApiError> {
        let mut version = Self::capture(p, label, recovery);
        let prod = narrative::load(s, p)?;
        let raw = serde_json::to_value(&prod).map_err(bad)?;
        for asset in &p.assets {
            if referenced(&raw, asset.id) {
                version.asset_refs.insert(
                    asset.id,
                    json!({"file_name":asset.file_name,"kind":asset.kind}),
                );
            }
        }
        version.schema_version = 2;
        version.narrative = Some(prod);
        version.seal();
        Ok(version)
    }
    pub(super) fn restore_production(
        &self,
        s: &AppState,
        current: &Project,
        next: &Project,
    ) -> Result<Production, ApiError> {
        let previous = narrative::load(s, current)?;
        let mut prod = self.narrative.clone().unwrap_or_else(|| previous.clone());
        prod.revision = previous.revision.max(prod.revision).saturating_add(1);
        // Jobs/exports remain in the global history. Their old callbacks must not
        // overwrite restored decisions when reconcile is next opened.
        prod.job_id = None;
        prod.export_job_id = None;
        for scene in &mut prod.scenes {
            scene.visual_job_id = None;
            scene.voice_job_id = None;
            if scene.status == "generating" {
                scene.status = "media_review".into();
            }
        }
        for reference in &mut prod.references {
            reference.job_id = None;
            reference.analysis_job_id = None;
        }
        if ["planning", "generating"].contains(&prod.status.as_str()) {
            prod.status = "script_review".into();
        }
        if self.narrative.is_none() {
            prod.script_approved = false;
            prod.assembled_signature = None;
            prod.status = "script_review".into();
            prod.last_error=Some("Versione legacy: piano corrente conservato, raccordi da revisionare prima dell’export guidato".into());
        }
        narrative::validate(next, &mut prod)?;
        Ok(prod)
    }
}
#[derive(Serialize, Deserialize)]
struct Intent {
    project: Project,
    narrative: Production,
    backup_id: Uuid,
    integrity: Uuid,
}
impl Intent {
    fn checksum(&self) -> Uuid {
        fingerprint(&(&self.project, &self.narrative, self.backup_id))
    }
}
pub(super) fn restore_pair(
    data: &FsPath,
    p: &Project,
    n: &Production,
    backup: Uuid,
) -> anyhow::Result<()> {
    ensure_ready(data, p.id).map_err(|e| anyhow::anyhow!(e.1))?;
    let root = store::project_dir(data, p.id);
    let mut intent = Intent {
        project: p.clone(),
        narrative: n.clone(),
        backup_id: backup,
        integrity: Uuid::nil(),
    };
    intent.integrity = intent.checksum();
    let path = root.join(PENDING);
    store::write_json(&path, &intent)?;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)?
        .sync_all()?;
    recover_pending(&root)
}
pub(crate) fn recover_pending(root: &FsPath) -> anyhow::Result<()> {
    let path = root.join(PENDING);
    if !path.is_file() {
        return Ok(());
    }
    let intent: Intent = store::read_json(&path)?;
    anyhow::ensure!(
        intent.integrity == intent.checksum()
            && store::directory_project_id(root) == Some(intent.project.id),
        "Ripristino pendente non valido, file conservato"
    );
    anyhow::ensure!(
        root.join("edit-versions")
            .join(format!("{}.json", intent.backup_id))
            .is_file(),
        "Backup del ripristino assente: nessun dato sostituito"
    );
    editor::validate_timeline(&intent.project, &intent.project.timeline)?;
    let mut n = intent.narrative;
    narrative::validate(&intent.project, &mut n).map_err(|e| anyhow::anyhow!(e.1))?;
    store::write_json(&root.join("project.json"), &intent.project)?;
    store::write_json(&root.join("narrative.json"), &n)?;
    for name in ["project.json", "narrative.json"] {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join(name))?
            .sync_all()?;
    }
    std::fs::remove_file(path)?; // Only this validated, project-local intent, after both writes.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn narrative_is_sealed_and_cannot_be_injected_into_legacy_snapshot() {
        let p = Project::new("Documentary".into(), None);
        let mut v = Version::capture(&p, "A".into(), false);
        let prod = Production {
            scenes: vec![narrative::Scene::default()],
            ..Default::default()
        };
        v.narrative = Some(prod.clone());
        assert!(v.preview(&p).is_err());
        v.schema_version = 2;
        v.seal();
        assert!(v.preview(&p).is_ok());
        v.narrative.as_mut().unwrap().continuity.established_facts = "Altered".into();
        assert!(v.preview(&p).is_err());
    }
    #[test]
    fn interrupted_pair_restore_rolls_forward_both_documents_and_retains_backup() {
        let data = std::env::temp_dir().join(format!("nl-version-recovery-{}", Uuid::new_v4()));
        let p = Project::new("Short film A".into(), None);
        store::save_project(&data, &p).unwrap();
        let root = store::project_dir(&data, p.id);
        let backup = Version::capture(&p, "B before restore".into(), true);
        persist(&data, &backup).unwrap();
        let prod = Production {
            title: "Recovered direction".into(),
            revision: 42,
            ..Default::default()
        };
        let mut intent = Intent {
            project: p.clone(),
            narrative: prod,
            backup_id: backup.id,
            integrity: Uuid::nil(),
        };
        intent.integrity = intent.checksum();
        store::write_json(&root.join(PENDING), &intent).unwrap();
        // Simulates a process interruption between the two file writes.
        store::write_json(&root.join("project.json"), &p).unwrap();
        store::write_json(&root.join("narrative.json"), &Production::default()).unwrap();
        assert!(ensure_ready(&data, p.id).is_err());
        let loaded = store::load_projects(&data).unwrap();
        assert!(loaded.contains_key(&p.id));
        let n: Production = store::read_json(&root.join("narrative.json")).unwrap();
        assert_eq!(n.revision, 42);
        assert_eq!(n.title, "Recovered direction");
        assert!(!root.join(PENDING).exists());
        assert!(directory(&data, p.id)
            .join(format!("{}.json", backup.id))
            .is_file());
        assert!(ensure_ready(&data, p.id).is_ok());
        assert!(data.starts_with(std::env::temp_dir()));
        assert!(data
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nl-version-recovery-"));
        std::fs::remove_dir_all(data).unwrap();
    }
}
