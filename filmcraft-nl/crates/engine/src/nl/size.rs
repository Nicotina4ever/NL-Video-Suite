//! Generated media sizes derive from the open sequence, never from a guess.
//!
//! Images are the sequence size or twice it (`nl.imageScale`: `1x` / `2x`); video is two thirds of
//! the sequence (a third smaller) or half (`nl.videoScale`: `2/3` / `1/2`). Sizes round to the
//! model's granularity (`nl.sizeMultiple`: 8 or 16 pixels). A command parameter overrides the
//! preference for one run, so the dialog can offer a choice without changing what everyone else
//! gets; when the sequence has no usable size the caller must fail and say so rather than invent
//! one.

use serde_json::Value;

use crate::Session;

/// What is being generated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Image,
    Video,
}

impl Mode {
    /// `image` or `video` from a command parameter; images are the default.
    pub fn from_params(p: &Value) -> Self {
        match p.get("resolutionMode").and_then(Value::as_str) {
            Some("video") => Mode::Video,
            _ => Mode::Image,
        }
    }
}

/// Round a pixel count to `step` (8 or 16) within the sizes the models accept.
pub fn round_to(value: f64, step: u32) -> u32 {
    let step = step.clamp(1, 64) as f64;
    ((value / step).round() * step).clamp(step, 16384.0) as u32
}

/// The generated size for a sequence of `width` × `height`, or `None` when the sequence is empty.
/// The aspect ratio is preserved: the width is rounded to the model grid, the height is derived
/// from it, and a square sequence stays square. Images can be 1× or 2×; video is 1/3 of the
/// sequence, never a guessed 16:9 default.
pub fn from_sequence(width: u32, height: u32, mode: Mode, image_multiplier: f64, video_divisor: f64, step: u32) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let scale = match mode {
        Mode::Image => image_multiplier.max(1.0),
        Mode::Video => 1.0 / video_divisor.max(1.0),
    };
    let w = round_to(width as f64 * scale, step);
    if width == height {
        return Some((w, w));
    }
    let h = round_to(w as f64 * height as f64 / width as f64, step);
    Some((w, h))
}

/// The size for the current sequence, with command parameters overriding the NL preferences.
///
/// Parameters: `resolutionMode` (`image`/`video`), `imageScale` (`1x`/`2x` or a number),
/// `videoScale` (`2/3`/`1/2`) and `sizeMultiple` (`8`/`16`).
pub fn from_params(s: &Session, p: &Value) -> Option<(u32, u32)> {
    let (width, height) = {
        let settings = s.active_sequence()?.settings.clone();
        (settings.width, settings.height)
    };
    let mode = Mode::from_params(p);
    let nl = &s.prefs.nl;
    let image_multiplier = match p.get("imageScale") {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(1.0).clamp(1.0, 4.0),
        Some(Value::String(v)) if v == "2x" => 2.0,
        Some(Value::String(v)) if v == "1x" => 1.0,
        _ => nl.image_multiplier(),
    };
    let video_divisor = match p.get("videoScale").and_then(Value::as_str) {
        Some("1/2") => 2.0,
        Some("2/3") => 1.5,
        Some("1/3") => 3.0,
        _ => nl.video_divisor(),
    };
    let step = match p.get("sizeMultiple").and_then(Value::as_str) {
        Some("16") => 16,
        Some("8") => 8,
        _ => nl.size_step(),
    };
    from_sequence(width, height, mode, image_multiplier, video_divisor, step)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> crate::Session {
        let mut session = crate::Session::default();
        session.execute("file.openDemoProject", serde_json::json!({})).unwrap();
        session
    }

    #[test]
    fn images_are_the_sequence_size_or_double_it() {
        assert_eq!(from_sequence(1920, 1080, Mode::Image, 1.0, 1.5, 8), Some((1920, 1080)));
        assert_eq!(from_sequence(1920, 1080, Mode::Image, 2.0, 1.5, 8), Some((3840, 2160)));
    }

    #[test]
    fn video_is_two_thirds_or_half_of_the_sequence() {
        assert_eq!(from_sequence(1920, 1080, Mode::Video, 1.0, 1.5, 8), Some((1280, 720)));
        // Half of 1080 is 540, which is not a multiple of 8: the model grid wins and the dialog
        // shows 544 rather than a size the workflow cannot run.
        assert_eq!(from_sequence(1920, 1080, Mode::Video, 1.0, 2.0, 8), Some((960, 544)));
        assert_eq!(from_sequence(2048, 1152, Mode::Video, 1.0, 2.0, 8), Some((1024, 576)));
    }

    #[test]
    fn sizes_round_to_the_model_granularity() {
        // 1000 / 1.5 = 666.7 → 664 with 8, 672 with 16; 1000 / 2 = 500 → 496 or 496.
        assert_eq!(from_sequence(1000, 1000, Mode::Video, 1.0, 1.5, 8), Some((664, 664)));
        assert_eq!(from_sequence(1000, 1000, Mode::Video, 1.0, 1.5, 16), Some((672, 672)));
        assert_eq!(from_sequence(1000, 1000, Mode::Image, 1.0, 1.5, 16), Some((1008, 1008)));
    }

    #[test]
    fn an_empty_sequence_has_no_size() {
        assert_eq!(from_sequence(0, 1080, Mode::Image, 1.0, 1.5, 8), None);
        assert_eq!(from_sequence(1920, 0, Mode::Video, 1.0, 1.5, 8), None);
    }

    #[test]
    fn parameters_override_the_preferences_for_one_run() {
        let mut session = demo();
        session.prefs.nl.image_scale = "2x".into();
        session.prefs.nl.video_scale = "1/2".into();
        session.prefs.nl.size_multiple = "16".into();
        let (w, h) = session.active_sequence().map(|q| (q.settings.width, q.settings.height)).unwrap();

        // No parameter: the preferences apply.
        let image = from_params(&session, &serde_json::json!({"resolutionMode": "image"})).unwrap();
        assert_eq!(image, (round_to(w as f64 * 2.0, 16), round_to(h as f64 * 2.0, 16)));
        let video = from_params(&session, &serde_json::json!({"resolutionMode": "video"})).unwrap();
        assert_eq!(video, (round_to(w as f64 / 2.0, 16), round_to(h as f64 / 2.0, 16)));

        // A parameter wins for this run and the preference is untouched.
        let one = from_params(&session, &serde_json::json!({"resolutionMode": "image", "imageScale": "1x"})).unwrap();
        assert_eq!(one, (round_to(w as f64, 16), round_to(h as f64, 16)));
        assert_eq!(session.prefs.nl.image_scale, "2x");
        let two_thirds = from_params(&session, &serde_json::json!({"resolutionMode": "video", "videoScale": "2/3"})).unwrap();
        assert_eq!(two_thirds, (round_to(w as f64 / 1.5, 16), round_to(h as f64 / 1.5, 16)));
    }

    #[test]
    fn a_project_without_a_sequence_reports_no_size() {
        let session = crate::Session::default();
        assert_eq!(from_params(&session, &serde_json::json!({})), None);
    }
}
