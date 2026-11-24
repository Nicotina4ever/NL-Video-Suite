//! Titles from the built-in motion graphics templates: one command that applies a template with
//! its control values, optionally gives it a responsive intro/outro, and reports the placed clip
//! with its controls. The NL Titles panel and the CLI/agents use the same path.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// `nl.titles.add` — apply a graphics template at the playhead (or `time`) with control values.
fn titles_add(s: &mut Session, p: &Value) -> Result<Value> {
    let template = p
        .get("template")
        .and_then(Value::as_str)
        .filter(|template| !template.trim().is_empty())
        .ok_or_else(|| bad_cmd("nl.titles.add", "need `template`: the id or name from graphics.template.list"))?;
    let values = p.get("values").filter(|values| values.is_object()).cloned().unwrap_or_else(|| json!({}));
    let mut apply = json!({"template": template, "values": values});
    if let Some(time) = p.get("time") {
        apply["time"] = time.clone();
    }
    if let Some(track) = p.get("track") {
        apply["track"] = track.clone();
    }
    let placed = s.execute("graphics.template.apply", apply)?;
    let clip = placed["clip"].as_u64().ok_or_else(|| bad_cmd("nl.titles.add", "the template was applied but no clip id came back"))?;
    let intro = p.get("introSeconds").and_then(Value::as_f64).filter(|seconds| *seconds >= 0.0);
    let outro = p.get("outroSeconds").and_then(Value::as_f64).filter(|seconds| *seconds >= 0.0);
    let responsive = if intro.is_some() || outro.is_some() {
        s.execute("graphics.setResponsiveTime", json!({"clip": clip, "introSeconds": intro.unwrap_or(0.5), "outroSeconds": outro.unwrap_or(0.5)}))?
    } else {
        Value::Null
    };
    let controls = s.execute("graphics.template.controls", json!({"clip": clip}))?;
    Ok(json!({
        "clip": clip,
        "template": placed["template"],
        "name": placed["name"],
        "responsive": responsive,
        "controls": controls["controls"],
    }))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "nl.titles.add",
        label: "NL Add Title",
        menu: &[],
        shortcut: None,
        params: r#"{"template":str,"values":obj?,"time":ticks?,"track":index?,"introSeconds":f?,"outroSeconds":f?"}"#,
        enabled: always,
        run: titles_add,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_placed_with_its_controls_and_responsive_time() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let placed = s
            .execute(
                "nl.titles.add",
                json!({
                    "template": "builtin:title-centered",
                    "values": {"title": "La voce prima di parlare", "subtitle": "Un doppiaggio"},
                    "introSeconds": 0.5,
                    "outroSeconds": 0.5
                }),
            )
            .unwrap();
        assert!(placed["clip"].as_u64().is_some());
        let controls = placed["controls"].as_array().unwrap();
        let title = controls.iter().find(|control| control["id"] == "title").unwrap();
        assert_eq!(title["value"], "La voce prima di parlare");
        assert!(placed["responsive"]["intro"].as_i64().unwrap_or(0) > 0);
    }

    #[test]
    fn an_unknown_template_is_refused() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        assert!(s.execute("nl.titles.add", json!({"template": "nope"})).is_err());
        assert!(s.execute("nl.titles.add", json!({})).is_err());
    }
}
