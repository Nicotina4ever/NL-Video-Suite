use crate::model::{AssetKind, MediaInfo, WorkflowSummary};
use anyhow::{anyhow, bail, Context};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

pub fn asset_kind_for(path: &Path) -> AssetKind {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => AssetKind::Video,
        "wav" | "mp3" | "m4a" | "aac" | "flac" | "ogg" | "opus" => AssetKind::Audio,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tiff" => AssetKind::Image,
        "srt" | "vtt" | "ass" | "sbv" => AssetKind::Subtitle,
        "txt" | "md" | "pdf" | "docx" => AssetKind::Document,
        _ => AssetKind::Other,
    }
}

pub fn safe_file_name(path: &Path) -> anyhow::Result<String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("the path does not contain a valid file name"))?;
    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        })
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        bail!("unsafe file name");
    }
    Ok(sanitized)
}

pub fn unique_destination(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let source = Path::new(name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let extension = source.extension().and_then(|value| value.to_str());
    for n in 2..10_000_u32 {
        let file_name = match extension {
            Some(extension) => format!("{stem}-{n}.{extension}"),
            None => format!("{stem}-{n}"),
        };
        let candidate = dir.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("asset-{}", Uuid::new_v4()))
}

pub fn copy_into_project(source: &Path, assets_dir: &Path) -> anyhow::Result<PathBuf> {
    if !source.is_file() {
        bail!("source is not an existing file: {}", source.display());
    }
    fs::create_dir_all(assets_dir)?;
    let destination = unique_destination(assets_dir, &safe_file_name(source)?);
    fs::copy(source, &destination)
        .with_context(|| format!("cannot import {}", source.display()))?;
    Ok(destination)
}

pub fn probe_media(path: &Path) -> anyhow::Result<Option<MediaInfo>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,width,height,r_frame_rate",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .context("could not start ffprobe; install FFmpeg or add it to PATH")?;
    if !output.status.success() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let duration_ms = value
        .pointer("/format/duration")
        .and_then(Value::as_str)
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| (seconds * 1000.0).round() as u64);
    let streams = value
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"));
    let has_audio = streams
        .iter()
        .any(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"));
    let frame_rate = video
        .and_then(|stream| stream.get("r_frame_rate"))
        .and_then(Value::as_str)
        .and_then(parse_frame_rate);
    Ok(Some(MediaInfo {
        duration_ms: if asset_kind_for(path) == AssetKind::Image {
            None
        } else {
            duration_ms
        },
        width: video
            .and_then(|stream| stream.get("width"))
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        height: video
            .and_then(|stream| stream.get("height"))
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        frame_rate,
        has_audio,
    }))
}

fn parse_frame_rate(text: &str) -> Option<f64> {
    let (numerator, denominator) = text.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator != 0.0).then_some(numerator / denominator)
}

pub fn extract_clip(input: &Path, output: &Path, start_ms: u64, end_ms: u64) -> anyhow::Result<()> {
    if end_ms <= start_ms {
        bail!("end_ms must be greater than start_ms");
    }
    let start = milliseconds_to_seconds(start_ms);
    let duration = milliseconds_to_seconds(end_ms - start_ms);
    let result = Command::new("ffmpeg")
        .args(["-y", "-ss", &start, "-i"])
        .arg(input)
        .args([
            "-t",
            &duration,
            "-map",
            "0:v:0?",
            "-map",
            "0:a:0?",
            "-c:v",
            "libx264",
            "-crf",
            "18",
            "-c:a",
            "aac",
            "-movflags",
            "+faststart",
        ])
        .arg(output)
        .output()
        .context("could not start ffmpeg; install FFmpeg or add it to PATH")?;
    if !result.status.success() {
        bail!("ffmpeg clip extraction failed: {}", stderr(&result.stderr));
    }
    Ok(())
}

pub fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn milliseconds_to_seconds(milliseconds: u64) -> String {
    format!("{:.3}", milliseconds as f64 / 1000.0)
}

fn stderr(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn scan_workflows(roots: &[PathBuf]) -> anyhow::Result<Vec<WorkflowSummary>> {
    let mut files = Vec::new();
    for root in roots {
        collect_json_files(root, &mut files)?;
    }
    let mut workflows = Vec::new();
    for file in files {
        if let Ok(bytes) = fs::read(&file) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                if let Some(summary) = summarise_workflow(&file, &value) {
                    workflows.push(summary);
                }
            }
        }
    }
    workflows.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(workflows)
}

fn collect_json_files(root: &Path, files: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root).with_context(|| format!("cannot scan {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if path.is_dir() {
            if !matches!(
                name.as_str(),
                "node_modules" | ".git" | "old" | "archive" | "_archive"
            ) {
                collect_json_files(&path, files)?;
            }
        } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
            files.push(path);
        }
    }
    Ok(())
}

fn summarise_workflow(path: &Path, value: &Value) -> Option<WorkflowSummary> {
    let current = std::env::current_dir().ok()?;
    let normalized = crate::runtime::native_path(path);
    let path = normalized.strip_prefix(&current).unwrap_or(&normalized);
    let nodes = value.as_object()?;
    if !nodes
        .values()
        .any(|n| n.get("class_type").and_then(Value::as_str).is_some())
    {
        return None;
    }
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    for node in nodes.values() {
        if let Some(title) = node_title(node) {
            if let Some(role) = tagged_role(title, "Input") {
                inputs.insert(role.to_owned());
            }
            if let Some(role) = tagged_role(title, "Output") {
                outputs.insert(role.to_owned());
            }
        }
    }
    Some(WorkflowSummary {
        id: Uuid::new_v5(&Uuid::NAMESPACE_URL, path.to_string_lossy().as_bytes()),
        name: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Unnamed workflow")
            .to_owned(),
        path: path.to_string_lossy().to_string(),
        input_roles: inputs.into_iter().collect(),
        output_roles: outputs.into_iter().collect(),
        node_count: nodes.len(),
    })
}

fn node_title(node: &Value) -> Option<&str> {
    node.pointer("/_meta/title")
        .and_then(Value::as_str)
        .or_else(|| node.get("title").and_then(Value::as_str))
}

fn tagged_role<'a>(title: &'a str, direction: &str) -> Option<&'a str> {
    let marker = format!("({direction}:");
    let start = title.find(&marker)? + marker.len();
    let end = title[start..].find(')')? + start;
    let role = title[start..end].trim();
    (!role.is_empty()).then_some(role)
}

pub fn patch_workflow_inputs(
    path: &Path,
    inputs: &std::collections::BTreeMap<String, Value>,
) -> anyhow::Result<Value> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read workflow {}", path.display()))?;
    let mut workflow: Value = serde_json::from_slice(&bytes).context("workflow JSON is invalid")?;
    let nodes = workflow
        .as_object_mut()
        .ok_or_else(|| anyhow!("workflow must be a ComfyUI API-format JSON object"))?;
    for (address, value) in inputs {
        let (node_id, input_name) = address
            .split_once('.')
            .ok_or_else(|| anyhow!("input '{address}' must use node_id.input_name"))?;
        let node = nodes
            .get_mut(node_id)
            .ok_or_else(|| anyhow!("workflow does not contain node '{node_id}'"))?;
        let node_inputs = node
            .get_mut("inputs")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow!("node '{node_id}' does not expose an inputs object"))?;
        node_inputs.insert(input_name.to_owned(), value.clone());
    }
    Ok(workflow)
}
