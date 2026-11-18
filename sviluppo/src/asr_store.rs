//! Optional private model containers. Never adopts or deletes external checkpoint paths.
use crate::{asr_catalog, storage, store, ApiError, AppState};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    sync::OnceLock,
};
use uuid::Uuid;
fn bad(e: impl ToString) -> ApiError {
    ApiError::bad_request(e.to_string())
}
fn lock() -> &'static tokio::sync::Mutex<()> {
    static L: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Entry {
    relative: PathBuf,
    bytes: u64,
    sha256: String,
    identity: Vec<u32>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    id: Uuid,
    creator: String,
    state: String,
    files: Vec<Entry>,
    plan: Option<Uuid>,
    #[serde(default)]
    plan_at: Option<chrono::DateTime<chrono::Utc>>,
    deleted: Vec<PathBuf>,
}
fn folder(s: &AppState, id: Uuid) -> PathBuf {
    s.data_dir.join("asr-managed").join(id.to_string())
}
fn load(s: &AppState, id: Uuid) -> Result<(PathBuf, Receipt), ApiError> {
    let root = folder(s, id);
    storage::reject_links(&root).map_err(bad)?;
    let receipt: Receipt = store::read_json(&root.join("ownership.json")).map_err(bad)?;
    if receipt.id != id
        || receipt.schema_version != 1
        || receipt.creator != "nl-suite-private-store-v1"
    {
        return Err(bad("Contenitore non creato dalla suite"));
    }
    Ok((root, receipt))
}
#[cfg(windows)]
fn info(file: &File) -> Result<Vec<u32>, ApiError> {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(h: *mut std::ffi::c_void, data: *mut u32) -> i32;
    }
    let mut value = [0u32; 13];
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), value.as_mut_ptr()) } == 0 {
        return Err(bad(std::io::Error::last_os_error()));
    }
    if value[10] != 1 {
        return Err(bad("Hardlink o numero di collegamenti non esclusivo"));
    }
    Ok(vec![value[7], value[11], value[12], value[10]])
}
#[cfg(not(windows))]
fn info(file: &File) -> Result<Vec<u32>, ApiError> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata().map_err(bad)?;
    if m.nlink() != 1 {
        return Err(bad("Hardlink escluso"));
    }
    Ok(vec![
        m.dev() as u32,
        m.ino() as u32,
        (m.ino() >> 32) as u32,
        1,
    ])
}
fn open(path: &Path) -> Result<File, ApiError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0).access_mode(0x80010000);
    }
    options.open(path).map_err(|e| {
        bad(format!(
            "File in uso, protetto o non accessibile: {} · {e}",
            path.display()
        ))
    })
}
fn scan(root: &Path) -> Result<Vec<(Entry, File)>, ApiError> {
    storage::reject_links(root).map_err(bad)?;
    let canonical = root.canonicalize().map_err(bad)?;
    let mut stack = vec![root.to_owned()];
    let mut files = vec![];
    let mut entries = 0;
    while let Some(path) = stack.pop() {
        entries += 1;
        if entries > 20_000 {
            return Err(bad(
                "Inventario troppo ampio: pulizia bloccata, nessun file eliminato",
            ));
        }
        storage::reject_links(&path).map_err(bad)?;
        if !path.canonicalize().map_err(bad)?.starts_with(&canonical) {
            return Err(bad("Risorsa fuori dal contenitore privato"));
        }
        if path.is_dir() {
            for entry in fs::read_dir(path).map_err(bad)? {
                stack.push(entry.map_err(bad)?.path());
            }
            continue;
        }
        if !path.is_file() {
            return Err(bad("Solo file regolari nel deposito privato"));
        }
        let relative = path.strip_prefix(root).map_err(bad)?.to_owned();
        let mut file = open(&path)?;
        let identity = info(&file)?;
        let bytes = file.metadata().map_err(bad)?.len();
        let sha256 = storage::handle_sha256(&mut file).map_err(bad)?;
        files.push((
            Entry {
                relative,
                bytes,
                sha256,
                identity,
            },
            file,
        ));
    }
    files.sort_by(|a, b| a.0.relative.cmp(&b.0.relative));
    Ok(files)
}
#[cfg(windows)]
fn mark_delete(file: &File, _path: &Path) -> Result<(), ApiError> {
    use std::os::windows::io::AsRawHandle;
    // FILE_DISPOSITION_INFO uses one BOOLEAN, not BOOL. Delete the verified handle,
    // not a reopened pathname. Microsoft Learn: /win32/api/winbase/ns-winbase-file_disposition_info
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            h: *mut std::ffi::c_void,
            class: i32,
            data: *const std::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    let value = 1u8;
    if unsafe {
        SetFileInformationByHandle(file.as_raw_handle(), 4, (&value as *const u8).cast(), 1)
    } == 0
    {
        return Err(bad(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(not(windows))]
fn mark_delete(_file: &File, path: &Path) -> Result<(), ApiError> {
    fs::remove_file(path).map_err(bad)
}
pub async fn view(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let base = s.data_dir.join("asr-managed");
    let mut rows = vec![];
    if base.exists() {
        storage::reject_links(&base).map_err(bad)?;
        for e in fs::read_dir(base).map_err(bad)? {
            let path = e.map_err(bad)?.path();
            let Some(id) = path
                .file_name()
                .and_then(|s| s.to_str())
                .and_then(|s| Uuid::parse_str(s).ok())
            else {
                continue;
            };
            let (_, r) = load(&s, id)?;
            let usage = asr_catalog::resource_usage(&s, &path.join("payload")).await?;
            rows.push(json!({"id":id,"path":crate::runtime::native_path(&path).join("payload"),"state":r.state,"files":r.files,"bytes":r.files.iter().map(|f|f.bytes).sum::<u64>(),"usage":usage,"deleted":r.deleted}));
        }
    }
    Ok(Json(
        json!({"containers":rows,"automatic_transfer":false,"notice":"Solo contenitori privati creati dalla suite e dichiarati esclusivi. Nessuna adozione di percorsi esterni o checkpoint preesistenti. Inventario SHA256, identità e hardlink ricontrollati prima di eliminare."}),
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    action: String,
    id: Option<Uuid>,
    plan_id: Option<Uuid>,
    confirmation: Option<String>,
}
pub async fn change(
    State(s): State<AppState>,
    Json(req): Json<Request>,
) -> Result<Json<Value>, ApiError> {
    let _guard = lock().lock().await;
    // Same mutation gate as catalog registration: no new profile can appear mid-delete.
    let _catalog = asr_catalog::lock().lock().await;
    if req.action == "create" {
        let id = Uuid::new_v4();
        let root = folder(&s, id);
        fs::create_dir_all(root.join("payload")).map_err(bad)?;
        storage::reject_links(&root).map_err(bad)?;
        let r = Receipt {
            schema_version: 1,
            id,
            creator: "nl-suite-private-store-v1".into(),
            state: "unsealed".into(),
            files: vec![],
            plan: None,
            plan_at: None,
            deleted: vec![],
        };
        store::write_json(&root.join("ownership.json"), &r).map_err(bad)?;
        return Ok(Json(
            json!({"id":id,"path":crate::runtime::native_path(&root).join("payload"),"copied":false,"notice":"Cartella vuota. Nessun peso copiato, spostato o scaricato. Non inserirvi modelli condivisi o riferiti da altri strumenti."}),
        ));
    }
    let id = req.id.ok_or_else(|| bad("ID contenitore richiesto"))?;
    let (root, mut receipt) = load(&s, id)?;
    let jobs = s.jobs.read().await;
    if jobs.values().any(|j| {
        matches!(
            j.status,
            crate::model::JobStatus::Queued | crate::model::JobStatus::Running
        )
    }) {
        return Err(bad(
            "Attendi la conclusione dei job: nessun modello eliminato",
        ));
    }
    let _permit = s
        .worker
        .clone()
        .try_acquire_owned()
        .map_err(|_| bad("Motore in uso"))?;
    let usage = asr_catalog::resource_usage(&s, &root.join("payload")).await?;
    if usage["complete"] != true || !usage["references"].as_array().is_some_and(|a| a.is_empty()) {
        return Err(bad(
            "Dipendenze, utilizzi storici o inventario incompleto: operazione bloccata",
        ));
    }
    if !matches!(req.action.as_str(), "seal" | "preview" | "delete") {
        return Err(bad("Operazione deposito non supportata"));
    }
    let files = scan(&root.join("payload"))?;
    let snapshot = files.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>();
    if req.action == "seal" {
        if req.confirmation.as_deref() != Some("USO ESCLUSIVO SUITE") {
            return Err(bad("Conferma la proprietà esclusiva: USO ESCLUSIVO SUITE"));
        }
        if snapshot.is_empty() {
            return Err(bad("Contenitore vuoto"));
        }
        receipt.files = snapshot;
        receipt.state = "sealed".into();
        receipt.plan = None;
        receipt.plan_at = None;
        store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
        return Ok(Json(
            json!({"sealed":true,"files":receipt.files,"ownership":"suite-private-user-confirmed"}),
        ));
    }
    if receipt.state != "sealed" || receipt.files != snapshot {
        return Err(bad(
            "Proprietà non confermata o file cambiati: inventario da verificare nuovamente",
        ));
    }
    if req.action == "preview" {
        let plan = Uuid::new_v4();
        receipt.plan = Some(plan);
        receipt.plan_at = Some(chrono::Utc::now());
        store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
        return Ok(Json(
            json!({"plan_id":plan,"files":receipt.files,"bytes":receipt.files.iter().map(|f|f.bytes).sum::<u64>(),"confirmation":"ELIMINA MODELLI","irreversible":true}),
        ));
    }
    if req.confirmation.as_deref() != Some("ELIMINA MODELLI")
        || req.plan_id.is_none()
        || receipt.plan != req.plan_id
        || receipt.plan_at.is_none_or(|at| {
            chrono::Utc::now() - at > chrono::Duration::minutes(10) || at > chrono::Utc::now()
        })
    {
        return Err(bad("Conferma o piano non validi"));
    }
    receipt.plan = None;
    receipt.state = "deleting".into();
    store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
    for (entry, file) in files {
        if let Err(e) = mark_delete(&file, &root.join("payload").join(&entry.relative)) {
            receipt.state = "sealed".into();
            store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
            return Err(e);
        }
        drop(file);
        receipt.deleted.push(entry.relative.clone());
        receipt.files.retain(|f| f.relative != entry.relative);
        store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
    }
    receipt.state = "deleted".into();
    store::write_json(&root.join("ownership.json"), &receipt).map_err(bad)?;
    drop(jobs);
    Ok(Json(
        json!({"deleted":receipt.deleted,"recoverable":false,"external_files_modified":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusive_handles_and_hardlinks_are_verified_on_real_fixture_files() {
        let root = std::env::temp_dir().join(format!("nl-owned-fixture-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("checkpoint fixture.gguf");
        fs::write(&path, b"GGUF").unwrap();
        let linked = root.join("duplicate.gguf");
        fs::hard_link(&path, &linked).unwrap();
        assert!(scan(&root).is_err());
        fs::remove_file(&linked).unwrap();
        let files = scan(&root).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0.bytes, 4);
        assert_eq!(files[0].0.identity.last(), Some(&1));
        let (entry, file) = files.into_iter().next().unwrap();
        assert_eq!(entry.relative, PathBuf::from("checkpoint fixture.gguf"));
        mark_delete(&file, &path).unwrap();
        drop(file);
        assert!(!path.exists());
        fs::remove_dir(&root).unwrap();
    }
}
