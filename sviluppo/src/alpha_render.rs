//! libass RGB + white coverage mask -> straight alpha, shared by PNG and WebM.
//! The subtitles filter's alpha=1 does not provide standard source-over coverage.
//! Keep RGB and coverage independent to avoid dim glow / double multiplication.
//! Bundled FFmpeg 9 negotiates alpha modes: mark existing premultiplied RGB explicitly
//! before unpremultiply, or an auto-converter multiplies it a second time.
//! Primary implementation reference: FFmpeg/libavfilter/vf_premultiply.c query_formats.
pub fn mask(script: &str) -> String {
    let mut out = script.to_owned();
    for prefix in ["\\c&H", "\\1c&H", "\\2c&H", "\\3c&H", "\\4c&H"] {
        let mut from = 0;
        while let Some(i) = out[from..].find(prefix) {
            let begin = from + i + prefix.len();
            if out.as_bytes().get(begin + 6) == Some(&b'&')
                && out[begin..begin + 6].bytes().all(|b| b.is_ascii_hexdigit())
            {
                out.replace_range(begin..begin + 6, "FFFFFF");
            }
            from = begin;
        }
    }
    // Our internal header has one style. Preserve alpha bytes, whiten RGB only.
    out.lines()
        .map(|line| {
            if !line.starts_with("Style: ") {
                return line.to_owned();
            }
            let mut fields = line.split(',').map(str::to_owned).collect::<Vec<_>>();
            for f in fields.iter_mut().take(7).skip(3) {
                if f.starts_with("&H") && f.len() == 10 {
                    f.replace_range(4..10, "FFFFFF");
                }
            }
            fields.join(",")
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn filter(
    ass: &str,
    mask: &str,
    size: (u32, u32),
    fps: &str,
    duration: f64,
    at: f64,
) -> String {
    filter_scaled(ass, mask, size, fps, duration, at, None)
}
pub fn filter_scaled(
    ass: &str,
    mask: &str,
    size: (u32, u32),
    fps: &str,
    duration: f64,
    at: f64,
    output: Option<(u32, u32)>,
) -> String {
    let source = format!(
        "color=c=black:s={}x{}:r={fps}:d={duration:.3},format=rgb24,settb=AVTB,setpts=PTS+{at:.3}/TB",
        size.0, size.1
    );
    let fonts = crate::fonts::directory_option();
    let scale = output
        .filter(|s| *s != size)
        .map(|s| format!(",scale={}:{}:flags=bicubic", s.0, s.1))
        .unwrap_or_default();
    format!("{source},subtitles=filename='{ass}'{fonts},format=gbrp[rgb];{source},subtitles=filename='{mask}'{fonts},format=gray[coverage];[rgb][coverage]alphamerge,setparams=alpha_mode=premultiplied,format=gbrap{scale},unpremultiply=inplace=1")
}
#[cfg(test)]
mod tests {
    #[test]
    fn coverage_whitens_colors_without_changing_opacity_or_text() {
        let source = "{\\alpha&H80&\\1a&HFF&\\c&H201015&\\3c&HFF3175&}Caffè";
        assert_eq!(
            super::mask(source),
            "{\\alpha&H80&\\1a&HFF&\\c&HFFFFFF&\\3c&HFFFFFF&}Caffè"
        );
        let graph = super::filter("a.ass", "a-mask.ass", (100, 100), "1", 1., 0.5);
        assert!(graph.contains("settb=AVTB"));
        assert!(graph.contains("alpha_mode=premultiplied"));
        assert!(graph.contains("PTS+0.500/TB"));
    }
}
