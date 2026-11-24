//! Graphic objects for the NL Graphics panel and automation: a solid or a text with a fill
//! colour, placed on the timeline at the playhead. One command wraps the graphics commands and
//! reports the placed clip, so the panel, CLI and agents follow the same path.

use serde_json::{Value, json};

use crate::commands::{CommandSpec, always, bad};
use crate::{EngineError, Result, Session};

fn bad_cmd(cmd: &str, msg: impl ToString) -> EngineError {
    bad(cmd, msg.to_string())
}

/// `nl.graphics.add` — rectangle, ellipse or text with a fill colour on the timeline.
fn graphics_add(s: &mut Session, p: &Value) -> Result<Value> {
    let kind = p.get("kind").and_then(Value::as_str).unwrap_or("rectangle").to_ascii_lowercase();
    if !matches!(kind.as_str(), "rectangle" | "ellipse" | "text") {
        return Err(bad_cmd("nl.graphics.add", "`kind` must be rectangle, ellipse or text"));
    }
    let position = p.get("position").cloned().filter(|position| position.is_array()).unwrap_or_else(|| json!([960.0, 540.0]));
    let seconds = p.get("seconds").and_then(Value::as_f64).filter(|seconds| *seconds > 0.0).unwrap_or(5.0);
    let created = if kind == "text" {
        s.execute(
            "graphics.newText",
            json!({
                "text": p.get("text").and_then(Value::as_str).unwrap_or("Title"),
                "position": position,
                "size": p.get("fontSize").and_then(Value::as_f64).unwrap_or(120.0),
                "seconds": seconds,
            }),
        )?
    } else {
        s.execute(
            "graphics.newShape",
            json!({
                "shape": kind,
                "position": position,
                "size": p.get("size").cloned().filter(|size| size.is_array()).unwrap_or_else(|| json!([400.0, 200.0])),
                "seconds": seconds,
            }),
        )?
    };
    let clip = created["clip"].as_u64().ok_or_else(|| bad_cmd("nl.graphics.add", "the graphic was created but no clip id came back"))?;
    let mut fill = Value::Null;
    if let Some(color) = p.get("fillColor").and_then(Value::as_str).filter(|color| !color.trim().is_empty()) {
        fill = s.execute("graphics.set", json!({"clip": clip, "props": {"fill_color": color}}))?;
    }
    Ok(json!({"clip": clip, "kind": kind, "fill": fill}))
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: "nl.graphics.add",
        label: "NL Add Graphic",
        menu: &[],
        shortcut: None,
        params: r##"{"kind":"rectangle|ellipse|text"?,"text":str?,"fontSize":f?,"position":[x,y]?,"size":[w,h]?,"fillColor":"#rrggbb"?,"seconds":f?}"##,
        enabled: always,
        run: graphics_add,
        journal: false,
        named: crate::commands::NamedKeys::NONE,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_and_text_are_created_with_a_fill() {
        let mut s = Session::default();
        s.execute("file.openDemoProject", json!({})).unwrap();
        let rectangle = s.execute("nl.graphics.add", json!({"kind": "rectangle", "fillColor": "#1d4ed8", "seconds": 2})).unwrap();
        assert!(rectangle["clip"].as_u64().is_some());
        let text = s.execute("nl.graphics.add", json!({"kind": "text", "text": "Ciao", "fillColor": "#ffffff"})).unwrap();
        assert!(text["clip"].as_u64().is_some());
        assert!(s.execute("nl.graphics.add", json!({"kind": "star"})).is_err());
    }
}
