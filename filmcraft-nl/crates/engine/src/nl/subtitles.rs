//! Subtitle files written from the project transcript: a normal SRT and the two-words-per-cue
//! uppercase "hormozi" variant, plus the ASS file the burn needs.
//!
//! Cues come from the word timings the recognizer aligned, never from estimated sentence lengths:
//! a clip's subtitles must stay on the words it actually contains. Timestamps are milliseconds and
//! monotonic by construction; an empty word range produces no cue rather than a zero-length one.

use std::fmt::Write as _;

use filmcraft_project::Word;

/// One subtitle cue in seconds relative to the clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Build cues from the words inside `[from, to)`, cutting the text at gaps and at `max_chars`.
pub fn cues_from_words(words: &[Word], from: f64, to: f64, max_chars: usize, max_gap: f64) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut current: Option<(f64, f64, String)> = None;
    for word in words {
        let start = word.start.seconds();
        let end = word.end.seconds();
        if end <= from || start >= to {
            continue;
        }
        let (start, end) = (start.max(from), end.min(to));
        let text = word.text.trim();
        if text.is_empty() || end <= start {
            continue;
        }
        match &mut current {
            Some((c_start, c_end, c_text)) => {
                let gap = start - *c_end;
                let too_long = c_text.chars().count() + 1 + text.chars().count() > max_chars;
                if gap > max_gap || too_long {
                    cues.push(Cue { start: *c_start - from, end: *c_end - from, text: c_text.clone() });
                    current = Some((start, end, text.to_string()));
                } else {
                    c_text.push(' ');
                    c_text.push_str(text);
                    *c_end = end;
                }
            }
            None => current = Some((start, end, text.to_string())),
        }
    }
    if let Some((start, end, text)) = current {
        cues.push(Cue { start: start - from, end: end - from, text });
    }
    // A cue ending exactly where the clip ends is fine; clamp any drift and keep order.
    cues.sort_by(|a, b| a.start.total_cmp(&b.start));
    cues
}

/// The "hormozi" variant: at most two words per cue, uppercase, no punctuation at the edges.
pub fn hormozi(cues: &[Cue]) -> Vec<Cue> {
    let mut out = Vec::new();
    for cue in cues {
        let words: Vec<&str> = cue.text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let per = words.len().div_ceil(2).max(1);
        let span = (cue.end - cue.start) / (per as f64);
        for (i, chunk) in words.chunks(per).enumerate() {
            let start = cue.start + span * i as f64;
            let end = if i + 1 == per { cue.end } else { cue.start + span * (i + 1) as f64 };
            out.push(Cue { start, end, text: chunk.join(" ").trim_matches(|c: char| c.is_ascii_punctuation()).to_uppercase() });
        }
    }
    out
}

/// Serialise cues as SRT (comma decimal separator, blank line between cues).
pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        let _ = writeln!(out, "{}", i + 1);
        let _ = writeln!(out, "{} --> {}", stamp(cue.start, ','), stamp(cue.end, ','));
        let _ = writeln!(out, "{}", cue.text);
        out.push('\n');
    }
    out
}

fn stamp(seconds: f64, separator: char) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

/// A minimal ASS document with explicit PlayRes, so libass scales the text for the clip, not for
/// an unknown reference frame (a plain SRT burn on a vertical clip renders oversized text).
pub fn to_ass(cues: &[Cue], width: u32, height: u32, font_size: u32) -> String {
    let mut out = String::new();
    let (w, h) = (width.max(16), height.max(16));
    out.push_str("[Script Info]\nScriptType: v4.00+\n");
    let _ = writeln!(out, "PlayResX: {w}\nPlayResY: {h}\nWrapStyle: 0\nScaledBorderAndShadow: yes\n");
    out.push_str("[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, OutlineColour, BackColour, Bold, Italic, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n");
    let _ = writeln!(out, "Style: Default,Arial Black,{font_size},&H0000FFFF,&H00000000,&H80000000,-1,0,1,3,0,2,60,60,{margin},1", margin = (h / 12).max(24));
    out.push_str("[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n");
    for cue in cues {
        let text = cue.text.replace('\n', "\\N");
        let _ = writeln!(out, "Dialogue: 0,{},{},Default,,0,0,0,,{}", ass_stamp(cue.start), ass_stamp(cue.end), text);
    }
    out
}

fn ass_stamp(seconds: f64) -> String {
    let cs = (seconds.max(0.0) * 100.0).round() as u64;
    let (h, m, s, cs) = (cs / 360_000, (cs / 6000) % 60, (cs / 100) % 60, cs % 100);
    format!("{h}:{m:02}:{s:02}.{cs:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use filmcraft_time::Tick;

    fn word(text: &str, start: f64, end: f64) -> Word {
        Word::new(text, Tick::from_seconds_f64(start), Tick::from_seconds_f64(end))
    }

    #[test]
    fn cues_split_at_gaps_and_keep_word_times() {
        let words = [word("ciao", 10.0, 10.4), word("a", 10.45, 10.55), word("tutti", 11.5, 11.9)];
        let cues = cues_from_words(&words, 10.0, 12.0, 40, 0.5);
        assert_eq!(cues.len(), 2, "{cues:?}");
        assert_eq!(cues[0].text, "ciao a");
        assert!((cues[0].start - 0.0).abs() < 1e-6);
        assert_eq!(cues[1].text, "tutti");
        assert!((cues[1].start - 1.5).abs() < 1e-6);
    }

    #[test]
    fn words_outside_the_clip_are_ignored_and_cues_stay_inside_it() {
        let words = [word("prima", 1.0, 1.5), word("dentro", 5.0, 5.5), word("dopo", 9.0, 9.5)];
        let cues = cues_from_words(&words, 4.0, 6.0, 40, 0.5);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "dentro");
        assert!(cues[0].start >= 0.0 && cues[0].end <= 2.0);
    }

    #[test]
    fn an_empty_range_writes_no_cue() {
        let words = [word("ciao", 10.0, 10.4)];
        assert!(cues_from_words(&words, 20.0, 30.0, 40, 0.5).is_empty());
        assert_eq!(to_srt(&[]), "");
    }

    #[test]
    fn srt_uses_monotonic_timestamps_and_a_comma() {
        let cues = [Cue { start: 0.0, end: 1.25, text: "uno".into() }, Cue { start: 1.25, end: 2.5, text: "due".into() }];
        let srt = to_srt(&cues);
        assert!(srt.contains("00:00:00,000 --> 00:00:01,250"));
        assert!(srt.contains("00:00:01,250 --> 00:00:02,500"));
        assert!(srt.contains("\n\n2\n"), "{srt}");
    }

    #[test]
    fn the_hormozi_variant_is_uppercase_and_short() {
        let cues = [Cue { start: 0.0, end: 2.0, text: "questo è un test".into() }];
        let short = hormozi(&cues);
        assert_eq!(short.len(), 2);
        assert!(short.iter().all(|c| c.text.chars().filter(|c| c.is_alphabetic()).all(|c| !c.is_lowercase())));
        assert!(short.iter().all(|c| c.text.split_whitespace().count() <= 2));
        // two cues cover the original span without overlapping the wrong way round
        assert!(short[0].start < short[1].start && short[0].end <= short[1].start + 1e-6);
    }

    #[test]
    fn ass_names_its_reference_frame() {
        let cues = [Cue { start: 0.0, end: 1.0, text: "ciao".into() }];
        let ass = to_ass(&cues, 1080, 1920, 72);
        assert!(ass.contains("PlayResX: 1080"));
        assert!(ass.contains("PlayResY: 1920"));
        assert!(ass.contains("Dialogue: 0,0:00:00.00,0:00:01.00,Default,,0,0,0,,ciao"));
    }
}
