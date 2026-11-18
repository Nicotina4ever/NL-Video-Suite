use crate::model::{AssetKind, Project, TimelineClip, TrackKind};
use anyhow::{bail, Context};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// A checked, immutable snapshot. Both export and preview render this exact timeline.
#[derive(Clone)]
pub struct Layer {
    pub path: PathBuf,
    pub kind: AssetKind,
    pub track: TrackKind,
    pub has_audio: bool,
    pub source_ratio: f64,
    pub clip: TimelineClip,
}
/// Isolated, half-open timeline window. Never changes saved clips or native/legacy animation phase.
pub fn window(project: &Project, from: u64, to: u64) -> Project {
    let mut p = project.clone();
    if p.render_audio_source.is_none() && crate::audio_mixer::processed(project) {
        p.render_audio_source = Some(crate::audio_mixer::Source {
            timeline: project.timeline.clone(),
            origin_ms: project.motion.timeline_origin_ms,
        });
    }
    p.motion.timeline_origin_ms += from;
    p.motion.render_duration_ms = Some(to.saturating_sub(from));
    p.timeline.clips = project
        .timeline
        .clips
        .iter()
        .filter_map(|clip| {
            let start = from.max(clip.start_ms);
            let end = to.min(clip.start_ms + clip.duration_ms);
            if start >= end {
                return None;
            }
            let mut c = clip.clone();
            c.transform.render_motion_origin_ms = Some(
                c.transform
                    .render_motion_origin_ms
                    .unwrap_or(c.start_ms as i64)
                    - from as i64,
            );
            c.transform.render_motion_duration_ms = Some(
                c.transform
                    .render_motion_duration_ms
                    .unwrap_or(c.duration_ms),
            );
            c.trim_in_ms += start - c.start_ms;
            c.start_ms = start - from;
            c.duration_ms = end - start;
            Some(c)
        })
        .collect();
    p
}

pub fn manifest(project: &Project, root: &Path) -> anyhow::Result<Vec<Layer>> {
    let mut layers = Vec::new();
    for track in &project.timeline.tracks {
        if !track.visible {
            continue;
        }
        let mut clips = project
            .timeline
            .clips
            .iter()
            .filter(|c| c.track_id == track.id)
            .collect::<Vec<_>>();
        clips.sort_by_key(|c| c.start_ms);
        for clip in clips {
            let asset = project
                .assets
                .iter()
                .find(|a| a.id == clip.asset_id)
                .context("La timeline fa riferimento a un asset inesistente")?;
            let path = root.join("assets").join(&asset.file_name);
            if !path.is_file() {
                bail!("Asset mancante: {}", path.display());
            }
            if !matches!(
                asset.kind,
                AssetKind::Video | AssetKind::Audio | AssetKind::Image
            ) {
                bail!("Asset non renderizzabile: {}", asset.name);
            }
            if clip.duration_ms == 0 || clip.start_ms.checked_add(clip.duration_ms).is_none() {
                bail!("Intervallo clip non valido");
            }
            layers.push(Layer {
                path,
                kind: asset.kind,
                track: track.kind,
                has_audio: asset.media.as_ref().is_some_and(|m| m.has_audio),
                source_ratio: asset
                    .media
                    .as_ref()
                    .and_then(|m| m.width.zip(m.height))
                    .filter(|(_, h)| *h > 0)
                    .map(|(w, h)| f64::from(w) / f64::from(h))
                    .unwrap_or(1.),
                clip: clip.clone(),
            });
        }
    }
    if layers.is_empty() && crate::motion::duration_ms(project) == 0 {
        bail!("La timeline è vuota");
    }
    Ok(layers)
}

pub fn dimensions(aspect: &str) -> anyhow::Result<(u32, u32)> {
    match aspect {
        "16:9" => Ok((1920, 1080)),
        "9:16" => Ok((1080, 1920)),
        "1:1" => Ok((1080, 1080)),
        "4:5" => Ok((1080, 1350)),
        "4:3" => Ok((1440, 1080)),
        _ => {
            let (w, h) = aspect.split_once(':').context("Formato non valido")?;
            let (w, h) = (w.parse::<u32>()?, h.parse::<u32>()?);
            anyhow::ensure!(
                (64..=4096).contains(&w)
                    && (64..=4096).contains(&h)
                    && w.is_multiple_of(2)
                    && h.is_multiple_of(2),
                "Formato custom: dimensioni pari 64–4096 pixel"
            );
            Ok((w, h))
        }
    }
}

/// Generated stills are authored above the delivery frame so the local model has
/// room for detail before the edit scales them down.
pub const IMAGE_SCALE: f64 = 1.5;
/// Latent image nodes need aligned sizes; 16 also covers /8 VAE downscales.
pub const IMAGE_STEP: u32 = 16;
/// A 4K source would otherwise ask for 5760 px: keep the ceiling explicit.
pub const IMAGE_MAX_SIDE: u32 = 4096;

fn aligned(value: u32) -> u32 {
    (((value as f64) / IMAGE_STEP as f64).round() as u32 * IMAGE_STEP).max(IMAGE_STEP)
}

/// 1.5x of the source frame, preserving its ratio and capped on the long side.
pub fn image_target(width: u32, height: u32) -> (u32, u32) {
    let (mut w, mut h) = (
        (width as f64 * IMAGE_SCALE).round() as u32,
        (height as f64 * IMAGE_SCALE).round() as u32,
    );
    let long = w.max(h);
    if long > IMAGE_MAX_SIDE {
        let ratio = IMAGE_MAX_SIDE as f64 / long as f64;
        w = (w as f64 * ratio).round() as u32;
        h = (h as f64 * ratio).round() as u32;
    }
    (aligned(w), aligned(h))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageBasis {
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
}

impl ImageBasis {
    /// Short explanation for the interface, e.g. "1,5× 1920×1080".
    pub fn label(&self) -> String {
        format!("1,5× {}×{}", self.source_width, self.source_height)
    }
}

fn video_size(asset: &crate::model::Asset) -> Option<(u32, u32)> {
    if asset.kind != AssetKind::Video {
        return None;
    }
    let media = asset.media.as_ref()?;
    Some((media.width?, media.height?))
}

/// Target picture size for a project: the explicitly linked video, otherwise the
/// largest project video, otherwise the project delivery format. No guessing
/// from filenames and no silent fallback below the request.
pub fn image_basis(project: &Project, explicit: Option<uuid::Uuid>) -> ImageBasis {
    let explicit_size = explicit
        .and_then(|id| project.assets.iter().find(|asset| asset.id == id))
        .and_then(video_size);
    let project_size = project
        .assets
        .iter()
        .filter_map(video_size)
        .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height));
    let source = explicit_size
        .or(project_size)
        .or_else(|| dimensions(&project.aspect_ratio).ok())
        .unwrap_or((1920, 1080));
    let (width, height) = image_target(source.0, source.1);
    ImageBasis {
        width,
        height,
        source_width: source.0,
        source_height: source.1,
    }
}

#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub fps: crate::time_map::FrameRate,
    pub crf: u8,
    pub audio_kbps: u32,
    pub codec: String,
    pub encoding: crate::export_encoding::Encoding,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            fps: Default::default(),
            crf: 18,
            audio_kbps: 192,
            codec: "h264".into(),
            encoding: Default::default(),
        }
    }
}
pub fn render_project(
    project: &Project,
    root: &Path,
    layers: &[Layer],
    output: &Path,
    size: (u32, u32),
    options: RenderOptions,
) -> anyhow::Result<()> {
    let prepared = if output.extension().is_none_or(|e| e != "png")
        && crate::audio_mixer::processed(project)
    {
        Some(crate::audio_mixer::master::prepare(
            project,
            root,
            options.audio_kbps,
        )?)
    } else {
        None
    };
    let plan = if crate::motion::hybrid::needed(project) {
        Some(crate::motion::hybrid::plan(project, size, root)?)
    } else {
        None
    };
    let ass = if plan.is_none() {
        crate::motion::overlay_script(project, size, root)?
    } else {
        None
    };
    render_styled(
        layers,
        output,
        size,
        ass.as_deref(),
        options,
        plan.as_ref(),
        project,
        prepared.as_ref(),
    )
}
#[allow(clippy::too_many_arguments)] // existing visual plan plus optional common audio receipt
fn render_styled(
    layers: &[Layer],
    output: &Path,
    size: (u32, u32),
    subtitles: Option<&str>,
    options: RenderOptions,
    hybrid: Option<&crate::motion::hybrid::Plan>,
    project: &Project,
    prepared: Option<&crate::audio_mixer::master::Prepared>,
) -> anyhow::Result<()> {
    options.fps.validate()?;
    anyhow::ensure!(
        (10..=51).contains(&options.crf) && (64..=384).contains(&options.audio_kbps),
        "Opzioni di render non valide"
    );
    let fps = format!("{}/{}", options.fps.numerator, options.fps.denominator);
    if layers.is_empty() && crate::motion::duration_ms(project) == 0 {
        bail!("Timeline vuota");
    }
    let (width, height) = size;
    let still = output.extension().is_some_and(|s| s == "png");
    let resolved = if still {
        None
    } else {
        Some(crate::export_encoding::resolve(
            &options.codec,
            &options.encoding,
            options.crf,
        )?)
    };
    let duration_ms = layers
        .iter()
        .map(|l| l.clip.start_ms + l.clip.duration_ms)
        .max()
        .unwrap_or(0)
        .max(crate::motion::duration_ms(project));
    let duration = duration_ms as f64 / 1000.0;
    let mut command = Command::new("ffmpeg");
    command.args([
        "-nostdin",
        "-n",
        "-v",
        "error",
        "-filter_complex_threads",
        "2",
    ]);
    let progress_path = output.with_extension("progress.txt");
    command
        .arg("-progress")
        .arg(&progress_path)
        .args(["-nostats", "-stats_period", "0.5"]);
    for layer in layers {
        command.args(["-threads", "2"]);
        if layer.kind == AssetKind::Image {
            command.args(["-loop", "1", "-framerate", &fps]);
        } else {
            command.args([
                "-ss",
                &format!("{:.3}", layer.clip.trim_in_ms as f64 / 1000.0),
            ]);
        }
        command
            .args([
                "-t",
                &format!("{:.3}", layer.clip.duration_ms as f64 / 1000.0),
            ])
            .arg("-i")
            .arg(&layer.path);
    }
    if let Some(mix) = &prepared {
        let origin = project
            .render_audio_source
            .as_ref()
            .map(|s| s.origin_ms)
            .unwrap_or(project.motion.timeline_origin_ms);
        command
            .args([
                "-ss",
                &format!(
                    "{:.3}",
                    (project.motion.timeline_origin_ms - origin) as f64 / 1000.
                ),
                "-t",
                &format!("{duration:.3}"),
                "-i",
            ])
            .arg(&mix.wav);
    }
    if let Some(plan) = hybrid {
        plan.inputs(&mut command, &fps);
    }
    let mut graph = format!("color=c=black:s={width}x{height}:r={fps}:d={duration:.3}[base0];anullsrc=r=48000:cl=stereo,atrim=duration={duration:.3}[silence]");
    let mut visual_count = 0;
    let mut audio_labels = vec!["[silence]".to_owned()];
    for (index, layer) in layers.iter().enumerate() {
        let clip = &layer.clip;
        let start = clip.start_ms as f64 / 1000.0;
        let end = (clip.start_ms + clip.duration_ms) as f64 / 1000.0;
        if layer.track != TrackKind::Audio && layer.kind != AssetKind::Audio {
            let transform = &clip.transform;
            transform.validate()?;
            let crop = transform.foreground_crop.as_ref();
            let (left, top, sx, sy) = crop
                .map(|c| (c.left, c.top, 1. - c.left - c.right, 1. - c.top - c.bottom))
                .unwrap_or((0., 0., 1., 1.));
            let fit = transform.resolved_fit(
                layer.source_ratio * sx / sy,
                f64::from(width) / f64::from(height),
            );
            let foreground_crop = if crop.is_some() {
                format!("format=rgba,crop=w='iw*{sx:.9}':h='ih*{sy:.9}':x='iw*{left:.9}':y='ih*{top:.9}':exact=1,")
            } else {
                String::new()
            };
            let scaled_width = ((width as f64 * transform.scale).round() as u32).max(2);
            let scaled_height = ((height as f64 * transform.scale).round() as u32).max(2);
            let sizing = if fit == "cover" {
                if let Some(path) = transform.camera_path.as_ref().filter(|p| p.enabled) {
                    let clock = format!("(t-{start:.6}+{:.6})", clip.trim_in_ms as f64 / 1000.);
                    let x = format!("(({})-{left:.9})/{sx:.9}", path.expression("x", &clock));
                    let y = format!("(({})-{top:.9})/{sy:.9}", path.expression("y", &clock));
                    // RGB crop exact=1 avoids chroma-grid jumps; normalize cadence first.
                    format!("fps={fps},scale={scaled_width}:{scaled_height}:force_original_aspect_ratio=increase:flags=lanczos,format=rgba,crop={scaled_width}:{scaled_height}:x='clip(iw*({x})-ow/2,0,iw-ow)':y='clip(ih*({y})-oh/2,0,ih-oh)':exact=1")
                } else {
                    format!("scale={scaled_width}:{scaled_height}:force_original_aspect_ratio=increase:flags=lanczos,crop={scaled_width}:{scaled_height}:x='(iw-ow)*{:.6}':y='(ih-oh)*{:.6}'",transform.focus_x,transform.focus_y)
                }
            } else {
                format!("scale={scaled_width}:{scaled_height}:force_original_aspect_ratio=decrease:flags=lanczos")
            };
            let rotation = if transform.rotation.abs() > 0.0001 {
                let radians = transform.rotation.to_radians();
                format!(",rotate={radians:.8}:ow=rotw({radians:.8}):oh=roth({radians:.8}):c=none")
            } else {
                String::new()
            };
            let motion_start = transform
                .render_motion_origin_ms
                .map(|v| v as f64 / 1000.)
                .unwrap_or(start);
            let motion_duration = transform
                .render_motion_duration_ms
                .map(|v| v as f64 / 1000.)
                .unwrap_or(end - start);
            let motion_end = motion_start + motion_duration;
            let ramp = 0.35_f64.min(motion_duration / 2.0).max(0.001);
            let explicit_fade = transform.fade_in_ms > 0 || transform.fade_out_ms > 0;
            let fade = if explicit_fade && transform.render_motion_origin_ms.is_none() {
                let mut filters = String::new();
                if transform.fade_in_ms > 0 {
                    filters.push_str(&format!(
                        ",fade=t=in:st={start:.6}:d={:.6}:alpha=1",
                        (transform.fade_in_ms as f64 / 1000.).min(motion_duration)
                    ));
                }
                if transform.fade_out_ms > 0 {
                    let ramp = (transform.fade_out_ms as f64 / 1000.).min(motion_duration);
                    filters.push_str(&format!(
                        ",fade=t=out:st={:.6}:d={ramp:.6}:alpha=1",
                        end - ramp
                    ));
                }
                filters
            } else if explicit_fade {
                let envelope = visual_envelope(transform, motion_duration, "T", motion_start);
                format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({envelope})'")
            } else if transform.motion != "none" && transform.render_motion_origin_ms.is_some() {
                format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*clip((T-{motion_start:.6})/{ramp:.6},0,1)*clip(({motion_end:.6}-T)/{ramp:.6},0,1)'")
            } else if transform.motion != "none" {
                format!(",fade=t=in:st={start:.3}:d={ramp:.3}:alpha=1,fade=t=out:st={:.3}:d={ramp:.3}:alpha=1",end-ramp)
            } else {
                String::new()
            };
            let phase = format!(
                "(pow(max(0,1-(t-{motion_start:.6})/{ramp:.6}),2)+pow(max(0,1-({motion_end:.6}-t)/{ramp:.6}),2))"
            );
            let dx = if transform.motion == "slide_left" {
                format!("-W*0.12*{phase}")
            } else {
                String::new()
            };
            let dy = if transform.motion == "slide_up" {
                format!("+H*0.12*{phase}")
            } else {
                String::new()
            };
            let progress = format!("clip((t-{motion_start:.6})/{motion_duration:.6},0,1)");
            let camera = if transform.zoom_start != 1.0 || transform.zoom_end != 1.0 {
                // Keep the layer dimensions fixed: overlay caches its input geometry.
                // Frame-varying scale changed pixels but left stale overlay bounds.
                // A transparent guard canvas plus uniform perspective is affine, not
                // a face warp. Standardize cadence before using the frame clock.
                let clock = format!(
                    "clip((on/({fps})+{start:.6}-{motion_start:.6})/{motion_duration:.6},0,1)"
                );
                let z = format!(
                    "({:.6}+({:.6})*{clock})",
                    transform.zoom_start,
                    transform.zoom_end - transform.zoom_start
                );
                let (cw, ch) = (
                    (scaled_width as f64 * 1.5).ceil() as u32,
                    (scaled_height as f64 * 1.5).ceil() as u32,
                );
                anyhow::ensure!(u64::from(cw)*u64::from(ch)<=33_554_432,"Camera fotografica: canvas oltre32Mpixel. Riduci scala/risoluzione; nessuna allocazione o riduzione silenziosa.");
                format!(",pad={cw}:{ch}:(ow-iw)/2:(oh-ih)/2:color=black@0,fps={fps},perspective=x0='W*(1-{z})/2':y0='H*(1-{z})/2':x1='W*(1+{z})/2':y1='H*(1-{z})/2':x2='W*(1-{z})/2':y2='H*(1+{z})/2':x3='W*(1+{z})/2':y3='H*(1+{z})/2':sense=destination:eval=frame")
            } else {
                String::new()
            };
            let pan_x = format!("+W*{:.6}*{progress}", transform.pan_x);
            let pan_y = format!("+H*{:.6}*{progress}", transform.pan_y);
            // Split ONE decoder: foreground and background share exact PTS/frame.
            // Overscan before blur prevents dark transparent edge halos.
            let foreground = if fit == "blur" {
                let sigma = (transform.background_blur * f64::from(width)).max(0.01);
                let padding = (sigma * 4.).ceil() as u32;
                let (bw, bh) = (width + padding * 2, height + padding * 2);
                graph.push_str(&format!(";[{index}:v]setpts=PTS-STARTPTS+{start:.3}/TB,split=2[fg{index}][bgin{index}];[bgin{index}]scale={bw}:{bh}:force_original_aspect_ratio=increase:flags=lanczos,format=rgba,setsar=1,crop={bw}:{bh},gblur=sigma={sigma:.5}:steps=3,crop={width}:{height}:{padding}:{padding},colorchannelmixer=rr={gain:.5}:gg={gain:.5}:bb={gain:.5}:aa={alpha:.5}{fade}[bg{index}];[base{visual_count}][bg{index}]overlay=eof_action=pass:repeatlast=0:enable='gte(t,{start:.3})*lt(t,{end:.3})'[base{next}]",gain=transform.background_brightness,alpha=clip.opacity*transform.background_intensity,next=visual_count+1));
                visual_count += 1;
                format!("[fg{index}]")
            } else {
                format!("[{index}:v]setpts=PTS-STARTPTS+{start:.3}/TB,")
            };
            graph.push_str(&format!(";{foreground}{foreground_crop}{sizing},format=rgba,setsar=1{camera}{rotation},colorchannelmixer=aa={:.4}{fade}[v{index}];[base{visual_count}][v{index}]overlay=x='W*{:.6}-w/2{dx}{pan_x}':y='H*{:.6}-h/2{dy}{pan_y}':eof_action=pass:repeatlast=0:enable='gte(t,{start:.3})*lt(t,{end:.3})'[base{}]",clip.opacity,transform.x,transform.y,visual_count+1));
            visual_count += 1;
        }
        if prepared.is_none() && layer.has_audio && clip.volume > 0.0 {
            graph.push(';');
            graph.push_str(&crate::audio_mixer::clip_filter(
                project, clip, index, true,
            )?);
            audio_labels.push(format!("[a{index}]"));
        }
    }
    let mut pass_files = vec![];
    let mut visual = format!("base{visual_count}");
    if let Some(plan) = hybrid {
        let directory = output.parent().context("Cartella render assente")?;
        command.current_dir(directory);
        let (last, files) = plan.filters(
            &mut graph,
            layers.len() + usize::from(prepared.is_some()),
            visual,
            size,
            &fps,
            duration,
            false,
            directory,
            uuid::Uuid::new_v4(),
        )?;
        visual = last;
        pass_files = files;
    }
    let mut ass_path = None;
    let subtitle_filter = if let Some(ass) = subtitles {
        let name = format!("caption-{}.ass", uuid::Uuid::new_v4());
        let directory = output.parent().context("Cartella export assente")?;
        let path = directory.join(&name);
        std::fs::write(&path, ass)?;
        command.current_dir(directory);
        ass_path = Some(path);
        format!(
            "format=rgb24,subtitles=filename='{name}'{},",
            crate::fonts::directory_option()
        )
    } else {
        String::new()
    };
    let pixel_format = if still { "rgb24" } else { "yuv420p" };
    if prepared.is_some() {
        graph.push_str(&format!(
            ";[silence]anullsink;[{}:a]asetpts=PTS-STARTPTS,atrim=duration={duration:.3}[audio]",
            layers.len()
        ));
    } else {
        graph.push_str(&format!(
            ";{}amix=inputs={}:normalize=0:duration=longest,atrim=duration={duration:.3}[audio]",
            audio_labels.join(""),
            audio_labels.len()
        ));
    }
    graph.push_str(&format!(
        ";[{visual}]{subtitle_filter}format={pixel_format}[video]"
    ));
    if still {
        graph.push_str(";[audio]anullsink");
    }
    let graph_path = output.with_extension("filters.txt");
    std::fs::write(&graph_path, graph)?;
    command.arg("-/filter_complex").arg(&graph_path);
    if still {
        command.args([
            "-map",
            "[video]",
            "-frames:v",
            "1",
            "-c:v",
            "png",
            "-threads",
            "2",
            "-update",
            "1",
        ]);
    } else {
        command.args([
            "-map",
            "[video]",
            "-map",
            "[audio]",
            "-t",
            &format!("{duration:.3}"),
            "-c:a",
            "aac",
            "-b:a",
            &format!("{}k", options.audio_kbps),
            "-movflags",
            "+faststart",
        ]);
        command.args(
            resolved
                .as_ref()
                .unwrap()
                .0
                .args(&options.codec, options.crf)?,
        );
        command.args(["-r", &fps]);
    }
    let result = command
        .arg(output)
        .output()
        .context("FFmpeg non avviabile")?;
    if !result.status.success() {
        bail!(
            "FFmpeg: {} (filtro salvato in {})",
            String::from_utf8_lossy(&result.stderr),
            graph_path.display()
        );
    }
    if let Some((effective, fallback)) = resolved {
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(output)
            .output()?;
        let actual: serde_json::Value = serde_json::from_slice(&probe.stdout)?;
        crate::store::write_json(
            &output.with_extension("encoding.json"),
            &serde_json::json!({"requested":{"codec":options.codec,"crf":options.crf,"encoding":options.encoding,"fps":options.fps,"audio_kbps":options.audio_kbps},"effective":effective,"fallback":fallback,"actual":actual,"inputs":layers.iter().map(|l|l.path.to_string_lossy()).collect::<Vec<_>>(),"proxy_substitution":false,"audio_normalized":prepared.is_some_and(|m|m.report["normalization_applied"]==true),"audio_master_receipt":prepared.map(|m|&m.report),"audio_delivered":if prepared.is_some(){Some(crate::audio_mixer::master::measure(output)?)}else{None},"audio_mixer":project.audio}),
        )?;
    }
    std::fs::remove_file(graph_path)?;
    if let Some(path) = ass_path {
        std::fs::remove_file(path)?;
    }
    for path in pass_files {
        std::fs::remove_file(path)?;
    }
    let _ = std::fs::remove_file(progress_path);
    Ok(())
}

pub(crate) fn envelope(enter: u64, exit: u64, duration: f64, clock: &str, origin: f64) -> String {
    let part = |ms: u64, numerator: String| {
        if ms == 0 {
            "1".to_owned()
        } else {
            format!(
                "clip(({numerator})/{:.6},0,1)",
                (ms as f64 / 1000.).min(duration)
            )
        }
    };
    format!(
        "{}*{}",
        part(enter, format!("{clock}-{origin:.6}")),
        part(exit, format!("{:.6}-{clock}", origin + duration))
    )
}
fn visual_envelope(
    t: &crate::model::VisualTransform,
    duration: f64,
    clock: &str,
    origin: f64,
) -> String {
    envelope(t.fade_in_ms, t.fade_out_ms, duration, clock, origin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Asset, MediaInfo};
    use chrono::Utc;
    use uuid::Uuid;

    fn video(width: u32, height: u32) -> Asset {
        Asset {
            id: Uuid::new_v4(),
            name: "clip".into(),
            kind: AssetKind::Video,
            file_name: "clip.mp4".into(),
            source_path: String::new(),
            imported_at: Utc::now(),
            media: Some(MediaInfo {
                duration_ms: Some(1000),
                width: Some(width),
                height: Some(height),
                frame_rate: Some(30.0),
                has_audio: true,
            }),
        }
    }

    #[test]
    fn image_target_is_one_and_a_half_and_aligned_to_sixteen() {
        // 1620 is not a multiple of 16: the aligned value is reported, never hidden.
        assert_eq!(image_target(1920, 1080), (2880, 1616));
        assert_eq!(image_target(1080, 1920), (1616, 2880));
        assert_eq!(image_target(720, 1280), (1088, 1920));
        assert_eq!(image_target(1440, 1080), (2160, 1616));
        for (width, height) in [(1920, 1080), (1080, 1920), (3840, 2160)] {
            let (w, h) = image_target(width, height);
            assert_eq!(w % IMAGE_STEP, 0);
            assert_eq!(h % IMAGE_STEP, 0);
        }
    }

    #[test]
    fn a_four_k_source_stops_at_the_long_side_ceiling() {
        assert_eq!(image_target(3840, 2160), (4096, 2304));
        let (w, h) = image_target(5760, 1080);
        assert_eq!(w.max(h), IMAGE_MAX_SIDE);
        assert!(h > 0);
    }

    #[test]
    fn explicit_video_wins_then_the_largest_then_the_delivery_format() {
        let mut project = Project::new("Test".into(), Some("16:9".into()));
        let small = video(640, 360);
        let large = video(2560, 1440);
        let explicit = video(1080, 1920);
        let explicit_id = explicit.id;
        project.assets.extend([small, large, explicit]);

        let basis = image_basis(&project, Some(explicit_id));
        assert_eq!((basis.width, basis.height), (1616, 2880));
        assert_eq!((basis.source_width, basis.source_height), (1080, 1920));

        let basis = image_basis(&project, None);
        assert_eq!((basis.source_width, basis.source_height), (2560, 1440));
        assert_eq!((basis.width, basis.height), (3840, 2160));

        let empty = Project::new("Test".into(), Some("9:16".into()));
        let basis = image_basis(&empty, None);
        assert_eq!((basis.source_width, basis.source_height), (1080, 1920));
        assert_eq!(basis.label(), "1,5× 1080×1920");
    }
}
