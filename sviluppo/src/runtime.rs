//! Installation diagnostics are deliberately distinct from feature parity.
use crate::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};

/// Some native AI libraries reject Win32 extended-path prefixes even when the
/// OS and Rust accept them. Keep process/model paths in ordinary Win32 form.
pub fn native_path(path: &std::path::Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(drive) = text.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(drive)
    } else {
        path.to_owned()
    }
}

/// The application root follows the executable, not the shell working folder.
/// Development binaries can still find the root through their ancestors.
pub fn initialize_environment() -> anyhow::Result<std::path::PathBuf> {
    let executable = std::env::current_exe()?;
    let root = if let Some(override_root) = std::env::var_os("NL_SUITE_ROOT") {
        std::path::PathBuf::from(override_root)
    } else if executable
        .parent()
        .and_then(std::path::Path::file_name)
        .is_some_and(|n| n == "app")
    {
        executable.parent().unwrap().parent().unwrap().to_owned()
    } else {
        executable
            .ancestors()
            .skip(1)
            .find(|p| p.join("runtime").is_dir() && p.join("workflows").is_dir())
            .map(std::path::Path::to_owned)
            .unwrap_or(std::env::current_dir()?)
    };
    let root = native_path(&root.canonicalize()?);
    anyhow::ensure!(
        root.join("runtime").is_dir() && root.join("engines").is_dir(),
        "Cartella applicazione incompleta: runtime/engines assenti"
    );
    std::env::set_current_dir(&root)?;
    let mut paths = vec![root.join("app").join("tools")];
    if let Some(current) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&current));
    }
    std::env::set_var("PATH", std::env::join_paths(paths)?);
    Ok(root)
}

pub fn portable_workflow_paths(workflows: &mut [crate::model::WorkflowSummary]) -> bool {
    let Ok(root) = std::env::current_dir() else {
        return false;
    };
    let mut changed = false;
    for workflow in workflows {
        if let Ok(path) = std::path::Path::new(&workflow.path).canonicalize() {
            let path = native_path(&path);
            if let Ok(relative) = path.strip_prefix(&root) {
                let value = relative.to_string_lossy().into_owned();
                if workflow.path != value {
                    workflow.path = value;
                    changed = true;
                }
            }
        }
    }
    changed
}

pub fn localize_workflows(
    config: &crate::model::AppConfig,
    workflows: &mut [crate::model::WorkflowSummary],
) -> bool {
    let mut changed = false;
    for workflow in workflows {
        for (system, child) in [
            ("regista", "workflow_comfy"),
            ("calliope", "example_ComfyUI_workflows"),
        ] {
            if let Some(old_root) = config.legacy_sources.get(system) {
                if let Ok(relative) =
                    std::path::Path::new(&workflow.path).strip_prefix(old_root.join(child))
                {
                    let local = std::path::Path::new("workflows")
                        .join(system)
                        .join(relative);
                    if local.is_file() {
                        if let Ok(local) = local.canonicalize() {
                            workflow.path = local.to_string_lossy().into_owned();
                            changed = true;
                        }
                    }
                }
            }
        }
    }
    changed
}

pub async fn status(State(state): State<AppState>) -> Json<Value> {
    let config = state.config();
    let python = &config.python_executable;
    let engines = &config.engine_root;
    let files = [
        "clipsai/motore.py",
        "clipsai/sgrossatore_raw.py",
        "clipsai/taglia_clip_per_editing.py",
        "clipsai/shorts.py",
        "zp_shorts/short.py",
    ];
    let missing: Vec<_> = files
        .iter()
        .filter(|name| !engines.join(name).is_file())
        .collect();
    let workflows = state.workflows.read().await;
    let legacy_workflows: Vec<_> = workflows
        .iter()
        .filter(|workflow| {
            config
                .legacy_sources
                .values()
                .any(|root| std::path::Path::new(&workflow.path).starts_with(root))
        })
        .map(|workflow| &workflow.path)
        .collect();
    Json(json!({
        "python_executable": python,
        "python_available": python.is_file(),
        "engine_root": engines,
        "missing_engine_files": missing,
        "engine_runtime_installed": python.is_file() && missing.is_empty(),
        "asr_model_available":config.asr_model_path.join("model.bin").is_file(),
        "italian_alignment_available":config.alignment_model_path.join("pytorch_model.bin").is_file(),
        "alignment_model_path":config.alignment_model_path,
        "asr_model_path":config.asr_model_path,
        "installation_root":std::env::current_dir().ok(),
        "legacy_workflow_references": legacy_workflows,
        "old_projects_safe_to_delete": false,
        "reason": "Runtime interno e parità funzionale sono verifiche diverse. Conserva gli originali finché non sono migrati e collaudati anche i flussi avanzati, i progetti e tutti i modelli/reference.",
        "external_services": ["Ollama", "ComfyUI"],
        "note": "ComfyUI può avere un proprio ambiente Python: la fusione con WhisperX/TTS richiede test di compatibilità; non viene effettuata automaticamente."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ai_paths_drop_only_windows_extended_prefixes() {
        assert_eq!(
            native_path(std::path::Path::new(r"\\?\K:\Video Suite\models")),
            std::path::PathBuf::from(r"K:\Video Suite\models")
        );
        assert_eq!(
            native_path(std::path::Path::new(r"\\?\UNC\server\share\models")),
            std::path::PathBuf::from(r"\\server\share\models")
        );
        assert_eq!(
            native_path(std::path::Path::new("models/asr")),
            std::path::PathBuf::from("models/asr")
        );
    }
}
