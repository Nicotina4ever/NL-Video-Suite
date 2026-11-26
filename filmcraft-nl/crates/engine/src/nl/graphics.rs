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


/// `nl.graphics.palette` — dominant colours of a reference image, for a coherent visual identity
/// across titles/cards. PNG only (the engine's image decoder ships the PNG codec).
fn palette(s: &mut Session, p: &Value) -> Result<Value> {
    let path = p.get("image").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).ok_or_else(|| bad_cmd("nl.graphics.palette", "serve `image`"))?;
    let resolved = if std::path::Path::new(path).is_file() { std::path::PathBuf::from(path) } else { super::models::find(s, path).ok_or_else(|| bad_cmd("nl.graphics.palette", format!("image not found: {path}")))? };
    let img = image::open(&resolved).map_err(|e| bad_cmd("nl.graphics.palette", format!("{}: {e}", resolved.display())))?.to_rgba8();
    let step = (img.width().max(img.height()) / 160).max(1) as u32;
    let mut counts: std::collections::HashMap<(u8, u8, u8), u32> = std::collections::HashMap::new();
    for y in (0..img.height()).step_by(step as usize) {
        for x in (0..img.width()).step_by(step as usize) {
            let [r, g, b, a] = img.get_pixel(x, y).0;
            if a < 32 { continue; }
            let key = ((r / 16) * 16, (g / 16) * 16, (b / 16) * 16);
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut colors: Vec<((u8, u8, u8), u32)> = counts.into_iter().collect();
    colors.sort_by(|a, b| b.1.cmp(&a.1));
    let hex = |(r, g, b): (u8, u8, u8)| format!("#{r:02x}{g:02x}{b:02x}");
    let palette: Vec<String> = colors.iter().take(8).map(|(c, _)| hex(*c)).collect();
    let accent = colors.iter().map(|(c, _)| *c).max_by_key(|(r, g, b)| {
        let max = *r.max(g).max(b) as i32;
        let min = *r.min(g).min(b) as i32;
        max - min
    }).map(hex).unwrap_or_else(|| "#ffffff".into());
    Ok(json!({"image": resolved.to_string_lossy(), "palette": palette, "accent": accent}))
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
    },
    CommandSpec {
        id: "nl.graphics.palette",
        label: "NL Graphics Palette",
        menu: &[],
        shortcut: None,
        params: r#"{"image":str}"#,
        enabled: always,
        run: palette,
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
