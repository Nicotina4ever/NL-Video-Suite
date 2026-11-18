//! Conditioning excerpts, independent of the original timeline master. No normalization.
use crate::{model::Asset, store, time_map::TimeRange, ApiError};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use uuid::Uuid;
/// LoadAudioUI receives an already extracted file. Its local clock must start
/// at zero; otherwise an old workflow trim would crop the excerpt a second time.
/// Only this verified loader contract is rebased, never arbitrary custom nodes.
pub fn rebase_loader_trim(
    graph: &mut Value,
    address: &str,
    range: TimeRange,
) -> anyhow::Result<Vec<Value>> {
    let (node, _) = address
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("Indirizzo reference non valido"))?;
    if graph[node]["class_type"] != "LoadAudioUI" {
        return Ok(vec![]);
    }
    let seconds = range.duration() as f64 / 1000.;
    let mut changes = vec![];
    for (field, after) in [
        ("start_time", 0.),
        ("end_time", seconds),
        ("duration", seconds),
    ] {
        if let Some(before) = graph[node]["inputs"].get(field).cloned() {
            anyhow::ensure!(before.is_number(),"LoadAudioUI {node}.{field}: il ritaglio locale deve essere numerico, non collegato, per utilizzare una finestra della sorgente");
            if before.as_f64() != Some(after) {
                changes.push(json!({"address":format!("{node}.{field}"),"before":before,"after":after,"reason":"Original source window already extracted; local excerpt clock, no second trim"}));
                graph[node]["inputs"][field] = json!(after);
            }
        }
    }
    Ok(changes)
}
pub fn validate(asset: &Asset, range: TimeRange) -> Result<(), ApiError> {
    TimeRange::new(range.start_ms, range.end_ms)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    if !asset
        .media
        .as_ref()
        .is_some_and(|m| m.has_audio && m.duration_ms.is_some_and(|d| range.end_ms <= d))
    {
        return Err(ApiError::bad_request(
            "Intervallo reference oltre la durata audio verificata della sorgente",
        ));
    }
    Ok(())
}
pub fn prepare(
    root: &Path,
    source: &Path,
    asset: Uuid,
    range: TimeRange,
) -> anyhow::Result<(PathBuf, Value)> {
    TimeRange::new(range.start_ms, range.end_ms)?;
    let stat = std::fs::metadata(source)?;
    let identity = json!({"contract":"audio-reference-source-rate-float32-v1","asset_id":asset,"source":crate::runtime::native_path(source),"source_bytes":stat.len(),"modified":stat.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_nanos().to_string(),"source_range":range,"audio_stream":0});
    let key = Uuid::new_v5(&asset, identity.to_string().as_bytes());
    let folder = root.join(".cache/references/audio");
    std::fs::create_dir_all(&folder)?;
    let output = folder.join(format!("{key}.wav"));
    let receipt = folder.join(format!("{key}.json"));
    if let Ok(mut saved) = store::read_json::<Value>(&receipt) {
        if saved["identity"] == identity
            && std::fs::metadata(&output)
                .is_ok_and(|m| Some(m.len()) == saved["wav_bytes"].as_u64())
        {
            saved["cache_hit"] = json!(true);
            return Ok((output, saved));
        }
    }
    let temporary = folder.join(format!("{key}-{}.partial.wav", Uuid::new_v4()));
    let mut command = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let result = command
        .args(["-nostdin", "-v", "error", "-threads", "2", "-ss"])
        .arg(format!("{:.3}", range.start_ms as f64 / 1000.))
        .arg("-i")
        .arg(crate::runtime::native_path(source))
        .arg("-t")
        .arg(format!("{:.3}", range.duration() as f64 / 1000.))
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_f32le"])
        .arg(crate::runtime::native_path(&temporary))
        .output()?;
    if !result.status.success() {
        let _ = std::fs::remove_file(&temporary);
        anyhow::bail!(
            "Estrazione reference audio: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let finalized = (|| {
        let info = crate::media::probe_media(&temporary)?
            .ok_or_else(|| anyhow::anyhow!("Reference audio non verificabile"))?;
        anyhow::ensure!(
            info.has_audio
                && info
                    .duration_ms
                    .is_some_and(|d| d.abs_diff(range.duration()) <= 2),
            "Durata estratto audio diversa dalla finestra richiesta"
        );
        std::fs::rename(&temporary, &output)?;
        let saved = json!({"key":key,"identity":identity,"wav_bytes":std::fs::metadata(&output)?.len(),"cache_hit":false,"format":"WAV float32; original sample rate/channels; first audio stream; no normalization","duration_ms":info.duration_ms});
        store::write_json(&receipt, &saved)?;
        Ok((output, saved))
    })();
    if finalized.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    finalized
}
