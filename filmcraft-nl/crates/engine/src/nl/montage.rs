//! Montage planner: a quick recut of the sequence's footage at a fixed shot length, planned first
//! and then applied — optionally into a brand-new sequence, so the current cut is never touched.
//!
//! The plan takes slices from the video clips already in the sequence, cycling through them and
//! advancing inside each clip, so the same few seconds of rushes can produce a longer montage. It is
//! deterministic: the same sequence, target and shot length give the same plan.

use serde_json::{Value, json};

use filmcraft_time::Tick;

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

const MAX_SHOTS: usize = 500;

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// One usable slice of footage: the media item and its source range.
struct Source {
    item: u64,
    name: String,
    start: Tick,
    len: Tick,
}

/// The video clips of the active sequence, with their source ranges.
fn sources(s: &Session) -> Vec<Source> {
    let Some(sequence) = s.active_sequence() else { return Vec::new() };
    let mut sources: Vec<Source> = Vec::new();
    for track in &sequence.video_tracks {
        for clip in &track.items {
            if !clip.enabled || clip.speed <= 0.0 {
                continue;
            }
            let len = Tick((clip.duration.0 as f64 * clip.speed.abs()).round() as i64);
            if len.0 <= 0 {
                continue;
            }
            sources.push(Source { item: clip.item.0, name: clip.name.clone(), start: clip.source_in, len });
        }
    }
    // A later clip of the same media is not extra material: keep one range per item, the longest.
    sources.sort_by_key(|source| std::cmp::Reverse(source.len.0));
    sources.dedup_by_key(|source| source.item);
    sources
}

/// Build a plan: shots of `shot_seconds`, taking slices in order and wrapping inside each source.
/// The rhythm of the shot lengths: steady, alternating long/short (pulse) or a short accelerando.
fn rhythm_factor(rhythm: &str, index: usize) -> f64 {
    match rhythm {
        "pulse" => {
            if index.is_multiple_of(2) {
                1.0
            } else {
                0.5
            }
        }
        "accelerando" => (1.4 - 0.25 * (index % 4) as f64).max(0.6),
        _ => 1.0,
    }
}

pub fn plan(s: &Session, target_seconds: f64, shot_seconds: f64, transition: &str, rhythm: &str) -> Result<Value> {
    let target = Tick::from_seconds_f64(target_seconds.clamp(0.5, 3600.0));
    let shot = Tick::from_seconds_f64(shot_seconds.clamp(0.2, 60.0));
    let sources = sources(s);
    if sources.is_empty() {
        return Err(bad_cmd("nl.montage.plan", "no video clips on the sequence: import footage first"));
    }
    // Shot lengths are computed in seconds so the rhythm can vary them; the last shot is trimmed
    // to end exactly on the target.
    let mut durations: Vec<f64> = Vec::new();
    let mut total = 0.0;
    let mut index = 0;
    while total < target.seconds() - 0.001 && durations.len() < MAX_SHOTS {
        let mut length = shot.seconds() * rhythm_factor(rhythm, index);
        let remaining = target.seconds() - total;
        if length > remaining {
            length = remaining;
        }
        length = (length * 1000.0).round() / 1000.0;
        if length <= 0.0 {
            break;
        }
        durations.push(length);
        total += length;
        index += 1;
    }
    let mut shots = Vec::new();
    let mut at = Tick::ZERO;
    let mut used: Vec<i64> = vec![0; sources.len()];
    for (index, length) in durations.iter().enumerate() {
        let source_index = index % sources.len();
        let source = &sources[source_index];
        let offset = used[source_index] % source.len.0.max(1);
        let wanted = Tick::from_seconds_f64(*length);
        let available = (source.len.0 - offset).max(wanted.0.min(source.len.0) / 2).max(1);
        let duration = Tick(wanted.0.min(available).max(1));
        used[source_index] += duration.0;
        shots.push(json!({
            "index": index + 1,
            "item": source.item,
            "name": source.name,
            "sourceInSeconds": (source.start.0 + offset) as f64 / filmcraft_time::TICKS_PER_SECOND as f64,
            "durationSeconds": duration.seconds(),
            "startSeconds": at.seconds(),
            "transition": transition,
        }));
        at = Tick(at.0 + duration.0);
    }
    Ok(json!({
        "shots": shots,
        "count": shots.len(),
        "targetSeconds": target.seconds(),
        "shotSeconds": shot.seconds(),
        "totalSeconds": at.seconds(),
        "sources": sources.len(),
        "transition": transition,
        "rhythm": if rhythm.is_empty() { "steady" } else { rhythm },
    }))
}

/// `nl.montage.plan` — plan a recut without touching the timeline.
fn montage_plan(s: &mut Session, p: &Value) -> Result<Value> {
    let target = p.get("targetSeconds").and_then(Value::as_f64).unwrap_or(30.0);
    let rhythm = p.get("rhythm").and_then(Value::as_str).unwrap_or("steady").to_ascii_lowercase();
    if !matches!(rhythm.as_str(), "steady" | "pulse" | "accelerando") {
        return Err(bad_cmd("nl.montage.plan", "`rhythm` must be steady, pulse or accelerando"));
    }
    let bpm = p.get("bpm").and_then(Value::as_f64).filter(|bpm| *bpm >= 20.0);
    let beats = p.get("beatsPerShot").and_then(Value::as_f64).filter(|beats| *beats > 0.0).unwrap_or(2.0);
    let shot = match bpm {
        Some(bpm) => 60.0 / bpm * beats,
        None => p.get("shotSeconds").and_then(Value::as_f64).unwrap_or(2.0),
    };
    let transition = p.get("transition").and_then(Value::as_str).unwrap_or("cut").to_ascii_lowercase();
    if !matches!(transition.as_str(), "cut" | "dissolve") {
        return Err(bad_cmd("nl.montage.plan", "`transition` must be cut or dissolve"));
    }
    let mut plan = plan(s, target, shot, &transition, &rhythm)?;
    if let Some(bpm) = bpm {
        plan["bpm"] = json!(bpm);
        plan["beatsPerShot"] = json!(beats);
    }
    s.nl.montage = Some(plan.clone());
    Ok(plan)
}

/// `nl.montage.apply` — place the planned shots, by default in a new sequence.
fn montage_apply(s: &mut Session, p: &Value) -> Result<Value> {
    let plan = p
        .get("plan")
        .cloned()
        .filter(|plan| plan.get("shots").is_some())
        .or_else(|| s.nl.montage.clone())
        .ok_or_else(|| bad_cmd("nl.montage.apply", "no plan yet: run nl.montage.plan first"))?;
    let Some(shots) = plan["shots"].as_array().cloned() else { return Err(bad_cmd("nl.montage.apply", "the plan has no shots")) };
    if shots.is_empty() {
        return Err(bad_cmd("nl.montage.apply", "the plan has no shots"));
    }
    let create = p.get("createSequence").and_then(Value::as_bool).unwrap_or(true);
    let name = p.get("name").and_then(Value::as_str).map(str::trim).filter(|name| !name.is_empty()).unwrap_or("Montage").to_string();
    let current = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let settings = current.settings.clone();
    if create {
        s.execute(
            "file.newSequence",
            json!({
                "name": name,
                "width": settings.width,
                "height": settings.height,
                "fps": settings.frame_rate.num as f64 / settings.frame_rate.den.max(1) as f64,
                "sampleRate": settings.sample_rate,
                "video": 1,
                "audio": 1,
            }),
        )?;
    }
    let mut placed = Vec::new();
    let mut errors = Vec::new();
    for shot in &shots {
        let params = json!({
            "item": shot["item"],
            "track": "V1",
            "time": Tick::from_seconds_f64(shot["startSeconds"].as_f64().unwrap_or(0.0)).0,
            "sourceIn": Tick::from_seconds_f64(shot["sourceInSeconds"].as_f64().unwrap_or(0.0)).0,
            "duration": Tick::from_seconds_f64(shot["durationSeconds"].as_f64().unwrap_or(1.0)).0,
            "video": true,
            "audio": p.get("audio").and_then(Value::as_bool).unwrap_or(false),
        });
        match s.execute("timeline.place", params) {
            Ok(result) => placed.push(json!({"shot": shot["index"], "clips": result["clips"], "item": shot["item"]})),
            Err(e) => errors.push(json!({"shot": shot["index"], "error": e.to_string()})),
        }
    }
    if placed.is_empty() {
        return Err(bad_cmd("nl.montage.apply", "the timeline refused every planned shot"));
    }
    let mut transitions = Vec::new();
    if plan["transition"] == "dissolve" || p.get("transition").and_then(Value::as_str) == Some("dissolve") {
        for entry in &placed {
            let Some(clips) = entry["clips"].as_array() else { continue };
            for clip in clips.iter().filter_map(Value::as_u64) {
                if let Ok(value) = s.execute("sequence.applyVideoTransition", json!({"clip": clip, "effect": "cross_dissolve", "frames": 12, "edge": "in"})) {
                    transitions.push(value);
                }
            }
        }
    }
    let sequence = s
        .active_sequence()
        .map(|sequence| json!({"width": sequence.settings.width, "height": sequence.settings.height, "durationSeconds": sequence.duration().seconds()}));
    Ok(json!({
        "placed": placed,
        "errors": errors,
        "transitions": transitions.len(),
        "newSequence": create,
        "sequence": sequence,
    }))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    let spec = |id: &'static str, label: &'static str, params: &'static str, run: fn(&mut Session, &Value) -> Result<Value>| -> CommandSpec {
        CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: always, run, journal: false, named: crate::commands::NamedKeys::NONE }
    };
    vec![
        spec(
            "nl.montage.plan",
            "NL Montage Plan",
            r##"{"targetSeconds":f?,"shotSeconds":f?,"bpm":f?,"beatsPerShot":f?,"rhythm":"steady|pulse|accelerando"?,"transition":"cut|dissolve"?"}"##,
            montage_plan,
        ),
        spec("nl.montage.apply", "NL Montage Apply", r#"{"plan":obj?,"createSequence":bool?,"name":str?,"audio":bool?}"#, montage_apply),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plan_covers_the_target_with_shots_of_the_requested_length() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let plan = s.execute("nl.montage.plan", json!({"targetSeconds": 6, "shotSeconds": 1.5, "transition": "dissolve"})).unwrap();
        assert_eq!(plan["count"], 4);
        assert!((plan["totalSeconds"].as_f64().unwrap() - 6.0).abs() < 0.01);
        let shots = plan["shots"].as_array().unwrap();
        assert!(shots.iter().all(|shot| shot["transition"] == "dissolve"));
        assert!(shots.windows(2).all(|pair| pair[0]["item"] != pair[1]["item"] || pair[0]["sourceInSeconds"] != pair[1]["sourceInSeconds"]));
    }

    #[test]
    fn a_pulse_rhythm_varies_the_shot_lengths_and_bpm_sets_the_beat() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let plan = s.execute("nl.montage.plan", json!({"targetSeconds": 4, "bpm": 120, "beatsPerShot": 2, "rhythm": "pulse"})).unwrap();
        assert_eq!(plan["bpm"], 120.0);
        assert!((plan["shotSeconds"].as_f64().unwrap() - 1.0).abs() < 0.001, "two beats at 120 bpm are one second");
        let durations: Vec<f64> = plan["shots"].as_array().unwrap().iter().map(|shot| shot["durationSeconds"].as_f64().unwrap()).collect();
        assert!(durations.len() > 2);
        assert!(durations.iter().any(|duration| (*duration - 1.0).abs() < 0.01));
        assert!(durations.iter().any(|duration| (*duration - 0.5).abs() < 0.01));
        let total: f64 = durations.iter().sum();
        assert!((total - 4.0).abs() < 0.05, "{total}");
    }

    #[test]
    fn applying_creates_a_new_sequence_and_places_every_shot() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let original = s.state.active_sequence.unwrap();
        s.execute("nl.montage.plan", json!({"targetSeconds": 3, "shotSeconds": 1, "transition": "cut"})).unwrap();
        let applied = s.execute("nl.montage.apply", json!({"name": "Montage test"})).unwrap();
        assert_eq!(applied["newSequence"], true);
        assert_eq!(applied["placed"].as_array().unwrap().len(), 3);
        let now = s.state.active_sequence.unwrap();
        assert_ne!(now, original);
        assert!(s.project.sequence(original).is_some(), "the original sequence is untouched");
    }

    #[test]
    fn applying_without_a_plan_is_refused() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.execute("nl.montage.apply", json!({})).is_err());
    }
}
