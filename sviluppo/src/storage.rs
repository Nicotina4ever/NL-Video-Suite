//! Conservative, suite-scoped maintenance. External model discovery is read-only.
use crate::{model::JobStatus, store, ApiError, AppState};
use axum::{extract::State, Json};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const PLAN_MINUTES: i64 = 15;
const MAX_CACHE_FILES: usize = 20_000;
const MAX_PLAN_BYTES: u64 = 12 * 1024 * 1024;
const SCAN_SECONDS: u64 = 15;
const OLD_LOG_SECONDS: u64 = 3 * 24 * 60 * 60;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewRequest {
    #[serde(default)]
    pub include_analysis_cache: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CacheFile {
    pub relative_path: PathBuf,
    pub category: String,
    pub bytes: u64,
    pub modified_secs: u64,
    pub modified_nanos: u32,
}

#[derive(Deserialize, Serialize)]
struct CleanupPlan {
    plan_id: Uuid,
    data_dir: PathBuf,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    include_analysis_cache: bool,
    files: Vec<CacheFile>,
}

#[derive(Default, Serialize)]
pub struct Category {
    pub category: String,
    pub files: usize,
    pub bytes: u64,
    pub description: String,
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub plan_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub total_bytes: u64,
    pub categories: Vec<Category>,
    pub files: Vec<CacheFile>,
    pub warnings: Vec<String>,
    pub protected: Vec<&'static str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanupRequest {
    pub plan_id: Uuid,
    pub confirmation: String,
}

#[derive(Serialize)]
pub struct CleanupEntry {
    pub relative_path: PathBuf,
    pub status: String,
    pub bytes: u64,
    pub reason: String,
}

#[derive(Serialize)]
pub struct CleanupResponse {
    pub plan_id: Uuid,
    pub deleted_files: usize,
    pub deleted_bytes: u64,
    pub skipped_files: usize,
    pub entries: Vec<CleanupEntry>,
    pub audit_path: PathBuf,
    pub note: &'static str,
}

pub async fn preview(
    State(state): State<AppState>,
    Json(request): Json<PreviewRequest>,
) -> Result<Json<PreviewResponse>, ApiError> {
    let data = state.data_dir.as_ref().clone();
    tokio::task::spawn_blocking(move || build_preview(&data, request.include_analysis_cache))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map(Json)
        .map_err(|e| ApiError::bad_request(e.to_string()))
}

pub async fn cleanup(
    State(state): State<AppState>,
    Json(request): Json<CleanupRequest>,
) -> Result<Json<CleanupResponse>, ApiError> {
    if request.confirmation != "ELIMINA CACHE" {
        return Err(ApiError::bad_request("Conferma richiesta: ELIMINA CACHE"));
    }
    // Keep the read guard until deletion finishes: new jobs cannot enter the queue.
    let jobs = state.jobs.read().await;
    if jobs
        .values()
        .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
    {
        return Err(ApiError::bad_request(
            "Attendi che la coda lavori sia vuota prima della pulizia",
        ));
    }
    let _permit = state
        .worker
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::bad_request("Un'elaborazione è attiva: pulizia non avviata"))?;
    let data = state.data_dir.as_ref().clone();
    let result = tokio::task::spawn_blocking(move || consume_plan(&data, request.plan_id))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map(Json)
        .map_err(|e| ApiError::bad_request(e.to_string()));
    drop(jobs);
    result
}

pub(crate) fn native_canonical(path: &Path) -> anyhow::Result<PathBuf> {
    reject_links(path)?;
    let canonical = crate::runtime::native_path(&path.canonicalize()?);
    reject_links(&canonical)?;
    Ok(canonical)
}

pub(crate) fn linked(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    false
}

pub(crate) fn reject_links(path: &Path) -> anyhow::Result<()> {
    // Check every ancestor, not only the final component: junctions can be parents.
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        if linked(&fs::symlink_metadata(ancestor)?) {
            anyhow::bail!("Link/junction escluso: {}", ancestor.display());
        }
    }
    Ok(())
}

fn data_root(data: &Path) -> anyhow::Result<PathBuf> {
    let data = native_canonical(data)?;
    let suite = native_canonical(&std::env::current_dir()?)?;
    if data == suite || !data.starts_with(&suite) {
        anyhow::bail!("Pulizia consentita solo nelle cartelle dati interne alla suite");
    }
    Ok(data)
}

fn relative_parts(path: &Path) -> Option<Vec<String>> {
    path.components()
        .map(|part| match part {
            Component::Normal(value) => value.to_str().map(str::to_owned),
            _ => None,
        })
        .collect()
}

fn category_for(relative: &Path, analysis: bool) -> Option<&'static str> {
    let p = relative_parts(relative)?;
    if p.len() >= 2 && p[0] == "previews" {
        return Some("previews");
    }
    if p.len() == 2 && p[0] == "logs" && p[1].ends_with(".log") {
        return Some("old_logs");
    }
    if p.len() >= 5
        && p[0] == "projects"
        && (Uuid::parse_str(&p[1]).is_ok()
            || p[1].rsplit_once("--").is_some_and(|(_, id)| {
                [8, 12, 16, 32].contains(&id.len()) && id.chars().all(|c| c.is_ascii_hexdigit())
            }))
    {
        if p[2] == ".cache" && ["previews", "text-metrics"].contains(&p[3].as_str()) {
            return Some("previews");
        }
        if analysis
            && p.len() == 5
            && p[2] == "analysis"
            && Uuid::parse_str(&p[3]).is_ok()
            && p[4].ends_with("_trascrizione.json")
        {
            return Some("analysis_cache");
        }
    }
    None
}

pub(crate) fn stamp(metadata: &Metadata) -> anyhow::Result<(u64, u32)> {
    let time = metadata.modified()?.duration_since(UNIX_EPOCH)?;
    Ok((time.as_secs(), time.subsec_nanos()))
}

fn old_log(metadata: &Metadata) -> bool {
    metadata
        .modified()
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age.as_secs() >= OLD_LOG_SECONDS)
}

struct CacheWalk<'a> {
    data: &'a Path,
    analysis: bool,
    visited: usize,
    start: Instant,
    files: Vec<CacheFile>,
    warnings: Vec<String>,
}

impl CacheWalk<'_> {
    fn walk(&mut self, path: &Path, depth: usize) {
        if self.visited >= MAX_CACHE_FILES || self.start.elapsed().as_secs() >= SCAN_SECONDS {
            return;
        }
        self.visited += 1;
        let Ok(metadata) = fs::symlink_metadata(path) else {
            self.warnings
                .push(format!("Non leggibile: {}", path.display()));
            return;
        };
        if linked(&metadata) {
            self.warnings
                .push(format!("Link/junction escluso: {}", path.display()));
            return;
        }
        if metadata.is_dir() && depth < 8 {
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    self.walk(&entry.path(), depth + 1);
                    if self.visited >= MAX_CACHE_FILES
                        || self.start.elapsed().as_secs() >= SCAN_SECONDS
                    {
                        break;
                    }
                }
            }
        } else if metadata.is_file() {
            let Ok(relative) = path.strip_prefix(self.data) else {
                return;
            };
            let Some(category) = category_for(relative, self.analysis) else {
                return;
            };
            if category == "old_logs" && !old_log(&metadata) {
                return;
            }
            if let Ok((modified_secs, modified_nanos)) = stamp(&metadata) {
                self.files.push(CacheFile {
                    relative_path: relative.to_path_buf(),
                    category: category.into(),
                    bytes: metadata.len(),
                    modified_secs,
                    modified_nanos,
                });
            }
        }
    }
}

pub(crate) fn safe_directory(data: &Path, relative: &Path) -> anyhow::Result<PathBuf> {
    if relative_parts(relative).is_none() {
        anyhow::bail!("Percorso manutenzione non valido");
    }
    let mut current = data.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.is_dir() && !linked(&meta) => {}
            Ok(_) => anyhow::bail!("Cartella manutenzione non sicura"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current)?,
            Err(e) => return Err(e.into()),
        }
    }
    let canonical = native_canonical(&current)?;
    if !canonical.starts_with(data) {
        anyhow::bail!("Cartella manutenzione fuori dai dati");
    }
    Ok(canonical)
}

fn build_preview(data: &Path, analysis: bool) -> anyhow::Result<PreviewResponse> {
    let data = data_root(data)?;
    let mut walk = CacheWalk {
        data: &data,
        analysis,
        visited: 0,
        start: Instant::now(),
        files: vec![],
        warnings: vec![
            "Sono esclusi modelli, runtime, media, esportazioni, workflow e progetti originali."
                .into(),
            "I log degli ultimi 3 giorni e le cartelle vuote non vengono rimossi.".into(),
        ],
    };
    for path in [data.join("previews"), data.join("logs")] {
        if path.exists() {
            walk.walk(&path, 0);
        }
    }
    // Never recurse through assets, exports or jobs just to discover cache files.
    let projects = data.join("projects");
    if projects.is_dir() && reject_links(&projects).is_ok() {
        if let Ok(entries) = fs::read_dir(&projects) {
            for entry in entries.flatten().take(MAX_CACHE_FILES) {
                if walk.visited >= MAX_CACHE_FILES || walk.start.elapsed().as_secs() >= SCAN_SECONDS
                {
                    break;
                }
                if store::directory_project_id(&entry.path()).is_none()
                    || reject_links(&entry.path()).is_err()
                {
                    continue;
                }
                for relative in [".cache/previews", ".cache/text-metrics", "analysis"] {
                    if relative == "analysis" && !analysis {
                        continue;
                    }
                    let path = entry.path().join(relative);
                    if path.exists() && reject_links(&path).is_ok() {
                        walk.walk(&path, 0);
                    }
                }
            }
        }
    }
    if walk.visited >= MAX_CACHE_FILES || walk.start.elapsed().as_secs() >= SCAN_SECONDS {
        walk.warnings.push(
            "Inventario parziale: limite di tempo/file raggiunto. Ripeti dopo la pulizia.".into(),
        );
    }
    if analysis {
        walk.warnings.push("Cache ASR inclusa su richiesta: la prossima analisi dovrà ricalcolarla. Le trascrizioni già salvate in project.json restano intatte.".into());
    }
    walk.files
        .sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let mut categories = BTreeMap::<String, Category>::new();
    for file in &walk.files {
        let category = categories
            .entry(file.category.clone())
            .or_insert_with(|| Category {
                category: file.category.clone(),
                description: match file.category.as_str() {
                    "previews" => "Anteprime rigenerabili: miniature e forme d'onda",
                    "old_logs" => "Log .log non recenti (almeno 3 giorni)",
                    _ => "Cache ASR opt-in; non le trascrizioni del progetto",
                }
                .into(),
                ..Category::default()
            });
        category.files += 1;
        category.bytes += file.bytes;
    }
    let now = Utc::now();
    let plan = CleanupPlan {
        plan_id: Uuid::new_v4(),
        data_dir: data.clone(),
        created_at: now,
        expires_at: now + ChronoDuration::minutes(PLAN_MINUTES),
        include_analysis_cache: analysis,
        files: walk.files.clone(),
    };
    let plans = safe_directory(&data, Path::new("maintenance/plans"))?;
    let bytes = serde_json::to_vec_pretty(&plan)?;
    if bytes.len() as u64 > MAX_PLAN_BYTES {
        anyhow::bail!("Inventario troppo grande: riduci i file di cache prima di riprovare");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(plans.join(format!("{}.json", plan.plan_id)))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(PreviewResponse {
        plan_id: plan.plan_id,
        expires_at: plan.expires_at,
        total_bytes: walk.files.iter().map(|f| f.bytes).sum(),
        categories: categories.into_values().collect(),
        files: walk.files,
        warnings: walk.warnings,
        protected: vec![
            "assets",
            "exports",
            "project.json",
            "models",
            "runtime",
            "engines",
            "workflows",
            "ZMateriale",
            "progetti esterni",
        ],
    })
}

fn validate_file(data: &Path, file: &CacheFile, analysis: bool) -> anyhow::Result<PathBuf> {
    let category = category_for(&file.relative_path, analysis)
        .ok_or_else(|| anyhow::anyhow!("Categoria/percorso non autorizzato"))?;
    if category != file.category {
        anyhow::bail!("Categoria modificata dopo l'anteprima");
    }
    let path = data.join(&file.relative_path);
    let canonical = native_canonical(&path)?;
    if !canonical.starts_with(data) || canonical == data {
        anyhow::bail!("Percorso fuori dai dati della suite");
    }
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || linked(&meta) {
        anyhow::bail!("Non è un file regolare");
    }
    if meta.len() != file.bytes || stamp(&meta)? != (file.modified_secs, file.modified_nanos) {
        anyhow::bail!("File cambiato dopo l'anteprima: genera un nuovo piano");
    }
    if category == "old_logs" && !old_log(&meta) {
        anyhow::bail!("Log recente: escluso");
    }
    Ok(path)
}

fn consume_plan(data: &Path, id: Uuid) -> anyhow::Result<CleanupResponse> {
    let data = data_root(data)?;
    let plans = safe_directory(&data, Path::new("maintenance/plans"))?;
    let path = plans.join(format!("{id}.json"));
    reject_links(&path)?;
    let meta = fs::metadata(&path)?;
    if !meta.is_file() || meta.len() > MAX_PLAN_BYTES {
        anyhow::bail!("Piano non valido");
    }
    let plan: CleanupPlan = serde_json::from_slice(&fs::read(&path)?)?;
    if plan.plan_id != id || plan.data_dir != data || plan.files.len() > MAX_CACHE_FILES {
        anyhow::bail!("Piano non valido per questa cartella dati");
    }
    if plan.expires_at < Utc::now()
        || plan.created_at > Utc::now()
        || plan.expires_at - plan.created_at > ChronoDuration::minutes(PLAN_MINUTES)
    {
        anyhow::bail!("Piano scaduto: aggiorna l'anteprima");
    }
    let audits = safe_directory(&data, Path::new("maintenance/audit"))?;
    let audit_path = audits.join(format!("{id}.jsonl"));
    let mut audit = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&audit_path)?;
    writeln!(
        audit,
        "{}",
        serde_json::json!({"event":"begin","plan_id":id,"at":Utc::now(),"file_count":plan.files.len()})
    )?;
    audit.sync_all()?;
    // Atomic claim makes a plan single-use, including a failed/partially skipped run.
    fs::rename(&path, plans.join(format!("{id}.consumed.json")))?;
    let mut response = CleanupResponse {
        plan_id: id,
        deleted_files: 0,
        deleted_bytes: 0,
        skipped_files: 0,
        entries: vec![],
        audit_path: PathBuf::from("maintenance/audit").join(format!("{id}.jsonl")),
        note: "Byte logici dei soli file effettivamente rimossi; lo spazio fisico recuperato può differire per compressione/hardlink. Nessun modello o file esterno viene eliminato.",
    };
    for file in &plan.files {
        writeln!(
            audit,
            "{}",
            serde_json::json!({"event":"attempt","relative_path":file.relative_path,"expected_bytes":file.bytes})
        )?;
        audit.flush()?;
        let result = validate_file(&data, file, plan.include_analysis_cache).and_then(|path| {
            // File-by-file deletion only: never remove_dir_all, never a user-supplied glob.
            // On Windows use an exclusive handle so an open log/cache is skipped.
            remove_checked(&data, &path, file)
        });
        let entry = match result {
            Ok(()) => {
                response.deleted_files += 1;
                response.deleted_bytes += file.bytes;
                CleanupEntry {
                    relative_path: file.relative_path.clone(),
                    status: "deleted".into(),
                    bytes: file.bytes,
                    reason: String::new(),
                }
            }
            Err(error) => {
                response.skipped_files += 1;
                CleanupEntry {
                    relative_path: file.relative_path.clone(),
                    status: "skipped".into(),
                    bytes: 0,
                    reason: error.to_string(),
                }
            }
        };
        writeln!(audit, "{}", serde_json::to_string(&entry)?)?;
        audit.flush()?;
        response.entries.push(entry);
    }
    writeln!(
        audit,
        "{}",
        serde_json::json!({"event":"complete","at":Utc::now(),"deleted_files":response.deleted_files,"deleted_bytes":response.deleted_bytes,"skipped_files":response.skipped_files})
    )?;
    audit.sync_all()?;
    Ok(response)
}

#[cfg(windows)]
pub(crate) fn remove_checked(data: &Path, path: &Path, expected: &CacheFile) -> anyhow::Result<()> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: u8, // Windows BOOLEAN, not BOOL.
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            class: i32,
            info: *const std::ffi::c_void,
            size: u32,
        ) -> i32;
        fn GetFinalPathNameByHandleW(
            handle: *mut std::ffi::c_void,
            output: *mut u16,
            length: u32,
            flags: u32,
        ) -> u32;
    }
    // DELETE access, no sharing, OPEN_REPARSE_POINT: no following a raced symlink.
    let file = OpenOptions::new()
        .access_mode(0x0001_0000 | 0x0080)
        .share_mode(0)
        .custom_flags(0x0020_0000)
        .open(path)?;
    let meta = file.metadata()?;
    if linked(&meta)
        || !meta.is_file()
        || meta.len() != expected.bytes
        || stamp(&meta)? != (expected.modified_secs, expected.modified_nanos)
    {
        anyhow::bail!("File modificato, aperto o non sicuro: saltato");
    }
    // Recheck the opened handle, not just the path: an ancestor could have been
    // replaced by a junction between validation and opening the file.
    let mut output = vec![0_u16; 32_768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            output.as_mut_ptr(),
            output.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= output.len() {
        anyhow::bail!("Impossibile verificare il percorso del file aperto");
    }
    let resolved = crate::runtime::native_path(&PathBuf::from(String::from_utf16(
        &output[..length as usize],
    )?));
    let relative = resolved
        .strip_prefix(data)
        .map_err(|_| anyhow::anyhow!("File aperto fuori dai dati della suite"))?;
    if relative != expected.relative_path {
        anyhow::bail!("Percorso cambiato durante la pulizia");
    }
    let info = FileDispositionInfo { delete_file: 1 };
    let result = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4,
            (&info as *const FileDispositionInfo).cast(),
            std::mem::size_of::<FileDispositionInfo>() as u32,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    drop(file); // Deletion takes effect on this exclusive handle's close.
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn remove_checked(
    _data: &Path,
    path: &Path,
    expected: &CacheFile,
) -> anyhow::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if linked(&meta)
        || meta.len() != expected.bytes
        || stamp(&meta)? != (expected.modified_secs, expected.modified_nanos)
    {
        anyhow::bail!("File modificato dopo l'anteprima");
    }
    fs::remove_file(path)?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelScanRequest {
    pub root: PathBuf,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default)]
    pub min_bytes: Option<u64>,
    #[serde(default)]
    pub hash_duplicates: bool,
    #[serde(default)]
    pub hash_budget_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct ModelFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub hash_status: String,
    #[serde(skip)]
    modified: (u64, u32),
}

#[derive(Serialize)]
pub struct DuplicateGroup {
    pub bytes_each: u64,
    pub paths: Vec<PathBuf>,
    pub status: &'static str,
    pub sha256: Option<String>,
}

#[derive(Serialize)]
pub struct ModelScanResponse {
    pub root: PathBuf,
    pub read_only: bool,
    pub files: Vec<ModelFile>,
    pub total_bytes: u64,
    pub visited_entries: usize,
    pub elapsed_ms: u128,
    pub truncated: bool,
    pub skipped_links: usize,
    pub duplicate_groups: Vec<DuplicateGroup>,
    pub warnings: Vec<String>,
}

pub async fn scan_models(
    Json(request): Json<ModelScanRequest>,
) -> Result<Json<ModelScanResponse>, ApiError> {
    tokio::task::spawn_blocking(move || model_inventory(request))
        .await
        .map_err(|e| ApiError::internal(e.into()))?
        .map(Json)
        .map_err(|e| ApiError::bad_request(e.to_string()))
}

struct ModelWalk {
    root: PathBuf,
    max_depth: usize,
    max_entries: usize,
    min_bytes: u64,
    start: Instant,
    visited: usize,
    skipped_links: usize,
    truncated: bool,
    files: Vec<ModelFile>,
    warnings: Vec<String>,
}

impl ModelWalk {
    fn limit(&self) -> bool {
        self.visited >= self.max_entries
            || self.files.len() >= 2_000
            || self.start.elapsed().as_secs() >= SCAN_SECONDS
    }

    fn walk(&mut self, path: &Path, depth: usize) {
        if self.limit() {
            self.truncated = true;
            return;
        }
        self.visited += 1;
        let Ok(meta) = fs::symlink_metadata(path) else {
            if self.warnings.len() < 30 {
                self.warnings
                    .push(format!("Non leggibile: {}", path.display()));
            }
            return;
        };
        if linked(&meta) {
            self.skipped_links += 1;
            return;
        }
        if reject_links(path).is_err() {
            self.skipped_links += 1;
            return;
        }
        let Ok(canonical) = native_canonical(path) else {
            return;
        };
        if !canonical.starts_with(&self.root) {
            self.skipped_links += 1;
            return;
        }
        if meta.is_dir() {
            if depth >= self.max_depth {
                self.truncated = true;
                return;
            }
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    self.walk(&entry.path(), depth + 1);
                    if self.limit() {
                        self.truncated = true;
                        break;
                    }
                }
            }
        } else if meta.is_file() && meta.len() >= self.min_bytes {
            let extension = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if ["gguf", "safetensors", "ckpt", "pt", "pth", "onnx", "bin"]
                .contains(&extension.as_str())
            {
                if let Ok(modified) = stamp(&meta) {
                    self.files.push(ModelFile {
                        path: path.to_path_buf(),
                        bytes: meta.len(),
                        sha256: None,
                        hash_status: "not_requested".into(),
                        modified,
                    });
                }
            }
        }
    }
}

fn model_inventory(request: ModelScanRequest) -> anyhow::Result<ModelScanResponse> {
    if request.root.as_os_str().is_empty() || !request.root.is_absolute() {
        anyhow::bail!("Scegli esplicitamente una cartella con percorso assoluto");
    }
    let raw = request.root.to_string_lossy();
    if raw.starts_with("\\\\") || raw.starts_with("//") {
        anyhow::bail!("Per questa scansione limitata scegli una cartella locale, non un percorso di rete/device");
    }
    let root = native_canonical(&request.root)?;
    if !root.is_dir() || root.parent().is_none() {
        anyhow::bail!("Scegli una sottocartella locale, non l'intero disco");
    }
    let mut walk = ModelWalk {
        root: root.clone(),
        max_depth: request.max_depth.unwrap_or(8).clamp(1, 16),
        max_entries: request.max_entries.unwrap_or(20_000).clamp(1, 50_000),
        min_bytes: request.min_bytes.unwrap_or(64 * 1024 * 1024).max(1),
        start: Instant::now(),
        visited: 0,
        skipped_links: 0,
        truncated: false,
        files: vec![],
        warnings: vec![],
    };
    walk.walk(&root, 0);
    let mut sizes = BTreeMap::<u64, Vec<usize>>::new();
    for (index, file) in walk.files.iter().enumerate() {
        sizes.entry(file.bytes).or_default().push(index);
    }
    let mut duplicate_groups = vec![];
    let mut hash_bytes = 0_u64;
    let budget = request
        .hash_budget_bytes
        .unwrap_or(512 * 1024 * 1024)
        .min(4 * 1024 * 1024 * 1024);
    for (size, indexes) in sizes.iter().filter(|(_, indexes)| indexes.len() > 1) {
        if request.hash_duplicates {
            for index in indexes {
                let file = &mut walk.files[*index];
                if hash_bytes.saturating_add(*size) > budget
                    || walk.start.elapsed().as_secs() >= SCAN_SECONDS
                {
                    file.hash_status = "skipped_budget_or_timeout".into();
                    continue;
                }
                hash_bytes += size;
                match hash_file(file, walk.start) {
                    Ok(hash) => {
                        file.sha256 = Some(hash);
                        file.hash_status = "verified".into();
                    }
                    Err(error) => {
                        file.hash_status = "unavailable_or_changed".into();
                        if walk.warnings.len() < 30 {
                            walk.warnings.push(error.to_string());
                        }
                    }
                }
            }
        }
        let mut hashes = BTreeMap::<String, Vec<PathBuf>>::new();
        for index in indexes {
            if let Some(hash) = &walk.files[*index].sha256 {
                hashes
                    .entry(hash.clone())
                    .or_default()
                    .push(walk.files[*index].path.clone());
            }
        }
        for (hash, paths) in hashes.into_iter().filter(|(_, paths)| paths.len() > 1) {
            duplicate_groups.push(DuplicateGroup {
                bytes_each: *size,
                paths,
                status: "sha256_identical",
                sha256: Some(hash),
            });
        }
        let unverified = indexes
            .iter()
            .filter(|i| walk.files[**i].sha256.is_none())
            .count();
        if unverified > 0 {
            duplicate_groups.push(DuplicateGroup {
                bytes_each: *size,
                paths: indexes
                    .iter()
                    .map(|i| walk.files[*i].path.clone())
                    .collect(),
                status: "same_size_unverified_not_proof",
                sha256: None,
            });
        }
    }
    walk.files
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.path.cmp(&b.path)));
    walk.warnings.push("Inventario in sola lettura: nessun modello viene spostato o eliminato. Stessa dimensione/nome non prova un duplicato; solo SHA-256 uguali lo confermano.".into());
    walk.warnings.push("Le estensioni sono indizi, non identificazione certa del modello. Il limite è 15 secondi/50.000 voci/2.000 file; hash opzionali entro budget.".into());
    Ok(ModelScanResponse {
        root,
        read_only: true,
        total_bytes: walk.files.iter().map(|f| f.bytes).sum(),
        files: walk.files,
        visited_entries: walk.visited,
        elapsed_ms: walk.start.elapsed().as_millis(),
        truncated: walk.truncated,
        skipped_links: walk.skipped_links,
        duplicate_groups,
        warnings: walk.warnings,
    })
}

/// Streaming content identity for already validated, project-owned media.
/// Callers cache this against size+mtime; never hashes shared model directories.
pub(crate) fn media_sha256(path: &Path) -> anyhow::Result<String> {
    let mut input = File::open(path)?;
    handle_sha256(&mut input)
}
pub(crate) fn handle_sha256(input: &mut File) -> anyhow::Result<String> {
    let before = input.metadata()?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut bytes = 0;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
        bytes += n as u64;
    }
    let after = input.metadata()?;
    anyhow::ensure!(
        bytes == before.len()
            && after.len() == before.len()
            && after.modified()? == before.modified()?,
        "Media cambiato durante il calcolo SHA256"
    );
    Ok(hash.finish())
}
fn hash_file(file: &ModelFile, start: Instant) -> anyhow::Result<String> {
    reject_links(&file.path)?;
    let mut input = File::open(&file.path)?;
    let before = input.metadata()?;
    if linked(&before) || before.len() != file.bytes || stamp(&before)? != file.modified {
        anyhow::bail!(
            "File cambiato durante l'inventario: {}",
            file.path.display()
        );
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut read = 0_u64;
    loop {
        if start.elapsed() >= Duration::from_secs(SCAN_SECONDS) {
            anyhow::bail!("Budget hash esaurito");
        }
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        if read > file.bytes {
            anyhow::bail!("File cresciuto durante l'hash");
        }
        hash.update(&buffer[..count]);
    }
    let after = input.metadata()?;
    if read != file.bytes || after.len() != file.bytes || stamp(&after)? != file.modified {
        anyhow::bail!("File cambiato durante l'hash: {}", file.path.display());
    }
    Ok(hash.finish())
}

// Small streaming SHA-256, no external executable, network access or new dependency.
struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    used: usize,
    bytes: u64,
}
impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: [0; 64],
            used: 0,
            bytes: 0,
        }
    }
    fn update(&mut self, mut data: &[u8]) {
        self.bytes += data.len() as u64;
        if self.used > 0 {
            let take = (64 - self.used).min(data.len());
            self.buffer[self.used..self.used + take].copy_from_slice(&data[..take]);
            self.used += take;
            data = &data[take..];
            if self.used == 64 {
                self.compress(self.buffer);
                self.used = 0;
            }
        }
        while data.len() >= 64 {
            self.compress(data[..64].try_into().expect("64-byte block"));
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.used = data.len();
        }
    }
    fn compress(&mut self, block: [u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0_u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().expect("word"));
        }
        for i in 16..64 {
            let a = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let b = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(a)
                .wrapping_add(w[i - 7])
                .wrapping_add(b);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t = h
                .wrapping_add(s)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let ss = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t);
            d = c;
            c = b;
            b = a;
            a = t.wrapping_add(ss).wrapping_add(maj);
        }
        for (state, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(value);
        }
    }
    fn finish(mut self) -> String {
        let bits = self.bytes.wrapping_mul(8);
        self.update(&[0x80]);
        while self.used != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        self.state
            .iter()
            .map(|word| format!("{word:08x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::current_dir()
                .unwrap()
                .join(".storage-test-fixtures")
                .join(Uuid::new_v4().to_string());
            fs::create_dir_all(root.join("data")).unwrap();
            Self(root)
        }
        fn data(&self) -> PathBuf {
            self.0.join("data")
        }
        fn write(&self, relative: &Path, data: &[u8]) -> PathBuf {
            let path = self.data().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, data).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Exact UUID fixture only, never traverse symlinks or delete anything else.
            fn clear(path: &Path) {
                let Ok(meta) = fs::symlink_metadata(path) else {
                    return;
                };
                if meta.is_dir() && !linked(&meta) {
                    if let Ok(entries) = fs::read_dir(path) {
                        for entry in entries.flatten() {
                            clear(&entry.path());
                        }
                    }
                    let _ = fs::remove_dir(path);
                } else {
                    let _ = fs::remove_file(path);
                }
            }
            clear(&self.0);
            let _ = fs::remove_dir(self.0.parent().unwrap());
        }
    }
    #[test]
    fn scope_excludes_assets_exports_models_and_traversal() {
        let id = Uuid::new_v4();
        let asset = Uuid::new_v4();
        for path in [
            "../models/a.bin",
            "models/a.bin",
            "assets/a.png",
            "exports/out.mp4",
            "project.json",
            "previews/../../models/a.bin",
        ] {
            assert!(category_for(Path::new(path), true).is_none(), "{path}");
        }
        assert_eq!(
            category_for(Path::new("previews/x/a.jpg"), false),
            Some("previews")
        );
        let cache = format!("projects/{id}/analysis/{asset}/video_trascrizione.json");
        assert_eq!(
            category_for(
                Path::new(&format!("projects/{id}/.cache/text-metrics/test.json")),
                false
            ),
            Some("previews")
        );
        assert!(category_for(Path::new(&cache), false).is_none());
        assert_eq!(
            category_for(Path::new(&cache), true),
            Some("analysis_cache")
        );
    }
    #[test]
    fn preview_then_cleanup_is_single_use_and_leaves_media() {
        let f = Fixture::new();
        let preview = f.write(Path::new("previews/test/a.jpg"), b"thumbnail");
        let asset = f.write(Path::new("projects/asset.png"), b"real media");
        let model = f.write(Path::new("models/model.bin"), b"model");
        let plan = build_preview(&f.data(), false).unwrap();
        assert_eq!(plan.files.len(), 1);
        let result = consume_plan(&f.data(), plan.plan_id).unwrap();
        assert_eq!(result.deleted_files, 1);
        assert_eq!(result.deleted_bytes, 9);
        assert!(!preview.exists());
        assert!(asset.exists());
        assert!(model.exists());
        assert!(consume_plan(&f.data(), plan.plan_id).is_err());
    }
    #[test]
    fn changed_cache_is_skipped_and_new_files_are_not_deleted() {
        let f = Fixture::new();
        let first = f.write(Path::new("previews/a.jpg"), b"original");
        let plan = build_preview(&f.data(), false).unwrap();
        fs::write(&first, b"changed-size").unwrap();
        let fresh = f.write(Path::new("previews/new.jpg"), b"fresh");
        let result = consume_plan(&f.data(), plan.plan_id).unwrap();
        assert_eq!(result.deleted_files, 0);
        assert_eq!(result.skipped_files, 1);
        assert!(first.exists());
        assert!(fresh.exists());
    }
    #[test]
    fn expired_or_modified_plan_cannot_target_assets() {
        let f = Fixture::new();
        let cache = f.write(Path::new("previews/a.jpg"), b"cache");
        let asset = f.write(Path::new("assets/important.jpg"), b"image");
        let preview = build_preview(&f.data(), false).unwrap();
        let plan_path = f
            .data()
            .join("maintenance/plans")
            .join(format!("{}.json", preview.plan_id));
        let mut plan: CleanupPlan = serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        plan.files[0].relative_path = PathBuf::from("assets/important.jpg");
        let meta = fs::metadata(&asset).unwrap();
        (plan.files[0].modified_secs, plan.files[0].modified_nanos) = stamp(&meta).unwrap();
        plan.files[0].bytes = meta.len();
        fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
        let result = consume_plan(&f.data(), preview.plan_id).unwrap();
        assert_eq!(result.deleted_files, 0);
        assert_eq!(result.skipped_files, 1);
        assert!(cache.exists());
        assert!(asset.exists());

        let preview = build_preview(&f.data(), false).unwrap();
        let plan_path = f
            .data()
            .join("maintenance/plans")
            .join(format!("{}.json", preview.plan_id));
        let mut plan: CleanupPlan = serde_json::from_slice(&fs::read(&plan_path).unwrap()).unwrap();
        plan.created_at = Utc::now() - ChronoDuration::minutes(30);
        plan.expires_at = Utc::now() - ChronoDuration::minutes(15);
        fs::write(&plan_path, serde_json::to_vec(&plan).unwrap()).unwrap();
        assert!(consume_plan(&f.data(), preview.plan_id).is_err());
        assert!(cache.exists());
    }
    #[test]
    fn asr_opt_in_never_removes_saved_project_transcript() {
        let f = Fixture::new();
        let id = Uuid::new_v4();
        let aid = Uuid::new_v4();
        let cache = f.write(
            Path::new(&format!("projects/{id}/analysis/{aid}/a_trascrizione.json")),
            b"cached",
        );
        let project = f.write(
            Path::new(&format!("projects/{id}/project.json")),
            b"transcript persisted",
        );
        assert!(build_preview(&f.data(), false).unwrap().files.is_empty());
        let plan = build_preview(&f.data(), true).unwrap();
        consume_plan(&f.data(), plan.plan_id).unwrap();
        assert!(!cache.exists());
        assert!(project.exists());
    }
    #[test]
    fn sha256_known_vectors_and_chunk_boundaries() {
        let mut hash = Sha256::new();
        hash.update(b"a");
        hash.update(b"bc");
        assert_eq!(
            hash.finish(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Sha256::new().finish(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut hash = Sha256::new();
        for _ in 0..1000 {
            hash.update(&[b'a'; 1000]);
        }
        assert_eq!(
            hash.finish(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
    #[test]
    fn inventory_never_calls_same_size_files_identical_without_hash() {
        let f = Fixture::new();
        f.write(Path::new("models/a.gguf"), b"same");
        f.write(Path::new("models/b.gguf"), b"same");
        f.write(Path::new("models/c.gguf"), b"diff");
        let request = |hash_duplicates| ModelScanRequest {
            root: f.data().join("models"),
            max_depth: None,
            max_entries: None,
            min_bytes: Some(1),
            hash_duplicates,
            hash_budget_bytes: None,
        };
        let scan = model_inventory(request(false)).unwrap();
        assert_eq!(scan.files.len(), 3);
        assert_eq!(
            scan.duplicate_groups[0].status,
            "same_size_unverified_not_proof"
        );
        let scan = model_inventory(request(true)).unwrap();
        let confirmed = scan
            .duplicate_groups
            .iter()
            .find(|g| g.status == "sha256_identical")
            .unwrap();
        assert_eq!(confirmed.paths.len(), 2);
        assert!(confirmed
            .paths
            .iter()
            .all(|p| p.file_name().unwrap() != "c.gguf"));
        assert!(f.data().join("models/c.gguf").exists());
    }
    #[cfg(windows)]
    #[test]
    fn open_cache_file_is_skipped() {
        let f = Fixture::new();
        let path = f.write(Path::new("previews/a.jpg"), b"open");
        let plan = build_preview(&f.data(), false).unwrap();
        let handle = File::open(&path).unwrap();
        let result = consume_plan(&f.data(), plan.plan_id).unwrap();
        assert_eq!(result.deleted_files, 0);
        assert_eq!(result.skipped_files, 1);
        assert!(path.exists());
        drop(handle);
    }
}
