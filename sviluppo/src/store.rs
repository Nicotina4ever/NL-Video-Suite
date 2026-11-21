use crate::model::{AppConfig, Job, Project, WorkflowSummary};
use anyhow::Context;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Stable ownership is inside the directory; its readable label is not an ID.
pub fn directory_project_id(path: &Path) -> Option<Uuid> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_dir() || crate::storage::linked(&meta) {
        return None;
    }
    let marker = path.join(".project-id");
    if let Ok(meta) = fs::symlink_metadata(&marker) {
        if meta.is_file() && !crate::storage::linked(&meta) && meta.len() <= 80 {
            if let Ok(id) = fs::read_to_string(marker).ok()?.trim().parse() {
                return Some(id);
            }
        }
    }
    if let Ok(id) = path.file_name()?.to_str()?.parse() {
        return Some(id);
    }
    // A partial deletion may have removed the marker; retain a discoverable registry.
    let record = path.join("project.json");
    let meta = fs::symlink_metadata(&record).ok()?;
    if !meta.is_file() || crate::storage::linked(&meta) {
        return None;
    }
    #[derive(serde::Deserialize)]
    struct Identity {
        id: Uuid,
    }
    serde_json::from_reader::<_, Identity>(std::io::BufReader::new(fs::File::open(record).ok()?))
        .ok()
        .map(|p| p.id)
}

fn readable_name(title: &str) -> String {
    let mut name = String::new();
    for c in title.chars() {
        if name.chars().count() >= 32 {
            break;
        }
        if c.is_alphanumeric() || matches!(c, '-' | '_') {
            name.push(c);
        } else if !name.ends_with(' ') {
            name.push(' ');
        }
    }
    let name = name.trim().trim_matches(['.', ' ']);
    if name.is_empty() {
        "Progetto".into()
    } else {
        name.into()
    }
}

pub fn load_or_create_config(data_dir: &Path) -> anyhow::Result<AppConfig> {
    let path = data_dir.join("settings.json");
    if path.exists() {
        return read_json(&path).context("cannot read data/settings.json");
    }
    let config = AppConfig::default();
    write_json(&path, &config)?;
    Ok(config)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderIssue {
    pub folder: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FolderMigration {
    pub from: String,
    pub to: String,
}

fn issues_path(data_dir: &Path) -> PathBuf {
    data_dir.join("project-folder-issues.json")
}

/// Non-fatal folder problems surfaced in the UI instead of blocking startup.
pub fn folder_issues(data_dir: &Path) -> Vec<String> {
    let path = issues_path(data_dir);
    if !path.is_file() {
        return Vec::new();
    }
    read_json::<Vec<FolderIssue>>(&path)
        .map(|items| {
            items
                .into_iter()
                .map(|i| format!("Cartella «{}»: {}", i.folder, i.reason))
                .collect()
        })
        .unwrap_or_default()
}

fn write_issues(data_dir: &Path, issues: &[FolderIssue]) -> anyhow::Result<()> {
    let path = issues_path(data_dir);
    if issues.is_empty() {
        if path.is_file() {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    write_json(&path, issues)
}

/// Legacy `<uuid>` folders become readable `<title>--<short id>` folders.
/// Ownership stays inside `.project-id`, so a later title change never moves data.
/// Every rename is reversible: the receipt records the old name.
pub fn migrate_project_folders(data_dir: &Path) -> anyhow::Result<Vec<FolderMigration>> {
    let base = data_dir.join("projects");
    if !base.is_dir() {
        return Ok(Vec::new());
    }
    crate::storage::reject_links(&base)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        let meta = fs::symlink_metadata(entry.path())?;
        if meta.is_dir() && !crate::storage::linked(&meta) {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let mut done = Vec::new();
    for path in paths {
        let Some(name) = path.file_name().and_then(|s| s.to_str()).map(str::to_owned) else {
            continue;
        };
        if Uuid::parse_str(&name).is_err() {
            continue; // Already readable or not ours: never touch it.
        }
        let record = path.join("project.json");
        if !record.is_file() {
            continue;
        }
        let Ok(project) = read_json::<Project>(&record) else {
            continue;
        };
        if directory_project_id(&path) != Some(project.id) {
            continue; // A foreign or ambiguous folder stays exactly where it is.
        }
        let label = readable_name(&project.title);
        let simple = project.id.simple().to_string();
        let mut target = None;
        for length in [8usize, 12, 16, 32] {
            let candidate = base.join(format!("{label}--{}", &simple[..length]));
            if candidate.exists() {
                continue;
            }
            if fs::rename(&path, &candidate).is_ok() {
                target = Some(candidate);
                break;
            }
        }
        let Some(target) = target else {
            continue; // Left under its stable legacy name; the app still opens it.
        };
        let marker = target.join(".project-id");
        if fs::write(&marker, project.id.to_string()).is_err() {
            let _ = fs::rename(&target, &path);
            continue;
        }
        if let Some(to) = target.file_name().and_then(|s| s.to_str()) {
            done.push(FolderMigration {
                from: name,
                to: to.to_owned(),
            });
        }
    }
    if !done.is_empty() {
        write_json(
            &data_dir.join("project-folders-migration.json"),
            &serde_json::json!({
                "schema_version":1,
                "at":chrono::Utc::now(),
                "items":done,
            }),
        )?;
    }
    Ok(done)
}

pub fn load_projects(data_dir: &Path) -> anyhow::Result<BTreeMap<Uuid, Project>> {
    let projects_dir = data_dir.join("projects");
    fs::create_dir_all(&projects_dir)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(&projects_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() || crate::storage::linked(&entry.metadata()?) {
            continue;
        }
        paths.push(entry.path());
    }
    paths.sort();
    let mut issues = Vec::new();
    // Read each record once to detect duplicates before any pending restore runs,
    // so a copied folder can never block startup or overwrite the original.
    let mut candidates = BTreeMap::<Uuid, (PathBuf, Project)>::new();
    let mut deferred = Vec::new();
    for path in paths {
        let record = path.join("project.json");
        if !record.is_file() {
            continue;
        }
        let project: Project = match read_json(&record) {
            Ok(project) => project,
            // An interrupted restore can leave a truncated record next to a
            // valid pending intent. Recovery below is the authority there.
            Err(_) => {
                deferred.push(path);
                continue;
            }
        };
        let replace = match candidates.get(&project.id) {
            None => {
                candidates.insert(project.id, (path, project));
                continue;
            }
            Some((existing_path, existing)) => {
                let loser = if project.updated_at > existing.updated_at {
                    existing_path.clone()
                } else {
                    path.clone()
                };
                issues.push(FolderIssue {
                    folder: loser
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?")
                        .to_owned(),
                    reason: "copia dello stesso progetto: non caricata, nessun file sovrascritto"
                        .into(),
                });
                project.updated_at > existing.updated_at
            }
        };
        if replace {
            candidates.insert(project.id, (path, project));
        }
    }
    for path in deferred {
        crate::edit_versions::recover_pending(&path)?;
        let record = path.join("project.json");
        let project: Project = read_json(&record)
            .with_context(|| format!("cannot read project record {}", record.display()))?;
        if candidates.contains_key(&project.id) {
            issues.push(FolderIssue {
                folder: path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("?")
                    .to_owned(),
                reason: "copia dello stesso progetto: non caricata, nessun file sovrascritto"
                    .into(),
            });
            continue;
        }
        candidates.insert(project.id, (path, project));
    }
    let mut projects = BTreeMap::new();
    for (_, (path, _)) in candidates {
        crate::edit_versions::recover_pending(&path)?;
        let mut project: Project = read_json(&path.join("project.json"))
            .with_context(|| format!("cannot read project record {}", path.display()))?;
        crate::album::restore_checkpoints(&path, &mut project)?;
        if let Some(owner) = directory_project_id(&path) {
            if owner != project.id {
                issues.push(FolderIssue {
                    folder: path.file_name().and_then(|s| s.to_str()).unwrap_or("?").to_owned(),
                    reason: format!(
                        "il marcatore indica il progetto {owner} ma il record contiene {}; cartella non caricata",
                        project.id
                    ),
                });
                continue;
            }
        }
        projects.insert(project.id, project);
    }
    write_issues(data_dir, &issues)?;
    Ok(projects)
}

pub fn save_project(data_dir: &Path, project: &Project) -> anyhow::Result<()> {
    fs::create_dir_all(data_dir.join("projects"))?;
    crate::storage::reject_links(&data_dir.join("projects"))?;
    let mut project_dir = project_dir(data_dir, project.id);
    if !project_dir.exists() {
        let label = readable_name(&project.title);
        let id = project.id.simple().to_string();
        for length in [8, 12, 16, 32] {
            let candidate = data_dir
                .join("projects")
                .join(format!("{label}--{}", &id[..length]));
            match fs::create_dir(&candidate) {
                Ok(()) => {
                    fs::write(candidate.join(".project-id"), project.id.to_string())?;
                    project_dir = candidate;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if directory_project_id(&candidate) == Some(project.id) {
                        project_dir = candidate;
                        break;
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::ensure!(
            project_dir.exists(),
            "Impossibile assegnare una cartella univoca al progetto"
        );
    }
    crate::storage::reject_links(&project_dir)?;
    fs::create_dir_all(project_dir.join("assets"))?;
    fs::create_dir_all(project_dir.join("exports"))?;
    write_json(&project_dir.join("project.json"), project)
}

pub fn project_dir(data_dir: &Path, project_id: Uuid) -> PathBuf {
    let base = data_dir.join("projects");
    let legacy = base.join(project_id.to_string());
    if legacy.is_dir() {
        return legacy;
    }
    if let Ok(entries) = fs::read_dir(&base) {
        let short = project_id.simple().to_string()[..8].to_owned();
        let entries: Vec<_> = entries.flatten().collect();
        for entry in entries.iter().filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains(&format!("--{short}"))
        }) {
            if directory_project_id(&entry.path()) == Some(project_id) {
                return entry.path();
            }
        }
        // Also tolerate a manually changed label when the ownership marker remains.
        for entry in &entries {
            if directory_project_id(&entry.path()) == Some(project_id) {
                return entry.path();
            }
        }
    }
    legacy
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod project_folder_tests {
    use super::*;
    #[test]
    fn readable_folder_persists_and_does_not_duplicate_on_title_change() {
        let root = std::env::temp_dir().join(format!("nl-store-{}", Uuid::new_v4()));
        let mut p = Project::new("Mi ficco in GTA".into(), None);
        save_project(&root, &p).unwrap();
        let dir = project_dir(&root, p.id);
        assert!(dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Mi ficco in GTA--"));
        assert_eq!(directory_project_id(&dir), Some(p.id));
        p.title = "Altro titolo".into();
        save_project(&root, &p).unwrap();
        assert_eq!(project_dir(&root, p.id), dir);
        assert_eq!(load_projects(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_uuid_and_invalid_windows_names_remain_safe() {
        let root = std::env::temp_dir().join(format!("nl-store-{}", Uuid::new_v4()));
        let p = Project::new("Legacy".into(), None);
        let dir = root.join("projects").join(p.id.to_string());
        fs::create_dir_all(&dir).unwrap();
        write_json(&dir.join("project.json"), &p).unwrap();
        save_project(&root, &p).unwrap();
        assert_eq!(project_dir(&root, p.id), dir);
        assert_eq!(readable_name("../<>:*?"), "Progetto");
        assert!(!readable_name("C:\\video/test").contains([':', '/', '\\']));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_folders_migrate_once_with_receipt_and_keep_resolving() {
        let root = std::env::temp_dir().join(format!("nl-store-{}", Uuid::new_v4()));
        let p = Project::new("Mi ficco in GTA".into(), None);
        let legacy = root.join("projects").join(p.id.to_string());
        fs::create_dir_all(&legacy).unwrap();
        write_json(&legacy.join("project.json"), &p).unwrap();
        let done = migrate_project_folders(&root).unwrap();
        assert_eq!(done.len(), 1);
        let dir = project_dir(&root, p.id);
        assert!(dir
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("Mi ficco in GTA--"));
        assert_eq!(directory_project_id(&dir), Some(p.id));
        assert!(!legacy.exists());
        assert!(root.join("project-folders-migration.json").is_file());
        assert!(
            migrate_project_folders(&root).unwrap().is_empty(),
            "idempotent"
        );
        assert_eq!(load_projects(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn duplicate_folder_is_skipped_and_reported_never_overwritten() {
        let root = std::env::temp_dir().join(format!("nl-store-{}", Uuid::new_v4()));
        let base = root.join("projects");
        fs::create_dir_all(&base).unwrap();
        let mut older = Project::new("Copia vecchia".into(), None);
        older.updated_at -= chrono::Duration::days(2);
        let mut newer = older.clone();
        newer.title = "Copia recente".into();
        newer.updated_at = chrono::Utc::now();
        let a = base.join(format!("{}--{:08x}", older.title, 0u32));
        let b = base.join("Copia recente--deadbeef");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        write_json(&a.join("project.json"), &older).unwrap();
        write_json(&b.join("project.json"), &newer).unwrap();
        fs::write(a.join(".project-id"), older.id.to_string()).unwrap();
        fs::write(b.join(".project-id"), newer.id.to_string()).unwrap();
        let loaded = load_projects(&root).unwrap();
        assert_eq!(loaded.len(), 1, "startup must not fail or load twice");
        assert_eq!(
            loaded[&newer.id].title, "Copia recente",
            "most recently edited copy wins"
        );
        let issues = folder_issues(&root);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("non caricata"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn foreign_marker_is_reported_and_not_loaded() {
        let root = std::env::temp_dir().join(format!("nl-store-{}", Uuid::new_v4()));
        let p = Project::new("Identità ambigua".into(), None);
        let dir = root.join("projects").join("Cartella--deadbeef");
        fs::create_dir_all(&dir).unwrap();
        write_json(&dir.join("project.json"), &p).unwrap();
        fs::write(dir.join(".project-id"), Uuid::new_v4().to_string()).unwrap();
        assert!(load_projects(&root).unwrap().is_empty());
        assert_eq!(folder_issues(&root).len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}

pub fn load_workflows(data_dir: &Path) -> anyhow::Result<Vec<WorkflowSummary>> {
    let path = data_dir.join("workflows.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

pub fn save_workflows(data_dir: &Path, workflows: &[WorkflowSummary]) -> anyhow::Result<()> {
    write_json(&data_dir.join("workflows.json"), workflows)
}

pub fn save_jobs(data_dir: &Path, jobs: &[Job]) -> anyhow::Result<()> {
    write_json(&data_dir.join("jobs.json"), jobs)
}

pub fn load_jobs(data_dir: &Path) -> anyhow::Result<Vec<Job>> {
    let path = data_dir.join("jobs.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json(&path)
}

pub fn write_json<T: serde::Serialize + ?Sized>(path: &Path, value: &T) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes)?;
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}
