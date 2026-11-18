//! Ordered native text/vector and project-owned raster passes. Shared by preview, still and MP4.
use super::*;
use std::path::{Path as FsPath, PathBuf};

pub enum Pass {
    Ass(String),
    Image(Box<Image>),
}
pub struct Image {
    path: PathBuf,
    begin: u64,
    end: u64,
    corners: [Vec<f64>; 8],
    alpha: Vec<f64>,
    crop: Option<Crop>,
}
pub struct Plan {
    pub passes: Vec<Pass>,
}
pub fn needed(p: &Project) -> bool {
    p.motion
        .graphics
        .iter()
        .any(|g| g.approved && g.layers.iter().any(|l| l.kind == "image"))
}
fn script(p: &Project, size: (u32, u32), ids: &HashSet<Uuid>) -> String {
    let mut empty = p.clone();
    empty.subtitles.units.clear();
    let mut s = crate::subtitle_studio::ass(&empty, size, true);
    s.push_str(&events_selected(p, size, None, false, Some(ids)));
    s
}
pub fn plan(p: &Project, size: (u32, u32), root: &FsPath) -> anyhow::Result<Plan> {
    validate(&p.motion).map_err(|e| anyhow::anyhow!(e.1))?;
    render_budget(p, None)?;
    let mut p = p.clone();
    resolve_width_links(&mut p, size, root)?;
    let mut passes = vec![];
    let mut ids = HashSet::new();
    let fps = crate::social::frame_rate(&p);
    for g in p.motion.graphics.iter().filter(|g| g.approved) {
        let Some((origin, begin, end)) = span(&p, g) else {
            continue;
        };
        for l in ordered_layers(g) {
            if l.kind != "image" {
                ids.insert(l.id);
                continue;
            }
            if !ids.is_empty() {
                passes.push(Pass::Ass(script(&p, size, &ids)));
                ids.clear();
            }
            let a = p
                .assets
                .iter()
                .find(|a| Some(a.id) == l.asset_id && a.kind == AssetKind::Image)
                .ok_or_else(|| anyhow::anyhow!("Asset immagine della motion assente"))?;
            let assets = root.join("assets").canonicalize()?;
            let path = assets.join(&a.file_name).canonicalize()?;
            anyhow::ensure!(
                path.starts_with(&assets) && path.is_file(),
                "Immagine fuori dalla libreria progetto"
            );
            let count = ((end - begin) * u64::from(fps.numerator))
                .div_ceil(1000 * u64::from(fps.denominator));
            let mut corners: [Vec<f64>; 8] =
                std::array::from_fn(|_| Vec::with_capacity(count as usize));
            let mut alpha = vec![];
            for frame in 0..count {
                let at =
                    begin + frame * 1000 * u64::from(fps.denominator) / u64::from(fps.numerator);
                let k = world_pose(g, l, (at as i64 - origin).max(0) as u64, size, 0);
                let angle = -k.rotation.to_radians();
                let (s, c) = angle.sin_cos();
                for (i, (x, y)) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)]
                    .into_iter()
                    .enumerate()
                {
                    let dx = x * l.width * f64::from(size.0) * k.scale;
                    let dy = y * l.height * f64::from(size.1) * k.scale;
                    corners[i * 2].push(k.x * f64::from(size.0) + dx * c - dy * s);
                    corners[i * 2 + 1].push(k.y * f64::from(size.1) + dx * s + dy * c);
                }
                alpha.push(k.opacity);
            }
            passes.push(Pass::Image(Box::new(Image {
                path,
                begin,
                end,
                corners,
                alpha,
                crop: l.crop.clone(),
            })));
        }
    }
    if !ids.is_empty() {
        passes.push(Pass::Ass(script(&p, size, &ids)));
    }
    if p.subtitles.enabled {
        passes.push(Pass::Ass(crate::subtitle_studio::ass(&p, size, true)));
    }
    Ok(Plan { passes })
}
/// Balanced lookup: exact frame-quantized native poses, logarithmic expression depth.
fn expression(values: &[f64], offset: usize, variable: &str) -> String {
    if values.is_empty() {
        return "0".into();
    }
    if values.iter().all(|v| (*v - values[0]).abs() < 0.000001) {
        return format!("{:.6}", values[0]);
    }
    let mid = values.len() / 2;
    format!(
        "if(lt({variable},{}),{},{})",
        offset + mid,
        expression(&values[..mid], offset, variable),
        expression(&values[mid..], offset + mid, variable)
    )
}
impl Plan {
    pub fn inputs(&self, cmd: &mut Command, fps: &str) {
        for pass in &self.passes {
            if let Pass::Image(image) = pass {
                cmd.args([
                    "-threads",
                    "2",
                    "-loop",
                    "1",
                    "-framerate",
                    fps,
                    "-t",
                    &format!("{:.3}", (image.end - image.begin) as f64 / 1000.),
                    "-i",
                ])
                .arg(&image.path);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn filters(
        &self,
        graph: &mut String,
        input_start: usize,
        mut base: String,
        size: (u32, u32),
        fps: &str,
        duration: f64,
        transparent: bool,
        directory: &FsPath,
        token: Uuid,
    ) -> anyhow::Result<(String, Vec<PathBuf>)> {
        let mut input = input_start;
        let mut files = vec![];
        for (index, pass) in self.passes.iter().enumerate() {
            let next = format!("hybrid{index}");
            match pass {
                Pass::Ass(script) => {
                    let name = format!("motion-{token}-{index}.ass");
                    let path = directory.join(&name);
                    std::fs::write(&path, script)?;
                    files.push(path);
                    if transparent {
                        let mask_name = format!("motion-{token}-{index}-mask.ass");
                        let path = directory.join(&mask_name);
                        std::fs::write(&path, crate::alpha_render::mask(script))?;
                        files.push(path);
                        let filter =
                            crate::alpha_render::filter(&name, &mask_name, size, fps, duration, 0.)
                                .replace("[rgb]", &format!("[mrgb{index}]"))
                                .replace("[coverage]", &format!("[mcov{index}]"));
                        graph.push_str(&format!(";{filter}[mass{index}];[{base}][mass{index}]overlay=format=rgb:alpha=straight:eof_action=pass:repeatlast=0[{next}]"));
                    } else {
                        graph.push_str(&format!(
                            ";[{base}]format=rgb24,subtitles=filename='{name}'{}[{next}]",
                            crate::fonts::directory_option()
                        ));
                    }
                }
                Pass::Image(image) => {
                    // Bound processing to the union of the animated layer, not the whole delivery frame.
                    // A 288px logo must not run per-pixel alpha equations over 2M/8M pixels per frame.
                    let bound = |axis: usize, limit: u32| {
                        let lo = image
                            .corners
                            .iter()
                            .skip(axis)
                            .step_by(2)
                            .flatten()
                            .copied()
                            .fold(f64::INFINITY, f64::min);
                        let hi = image
                            .corners
                            .iter()
                            .skip(axis)
                            .step_by(2)
                            .flatten()
                            .copied()
                            .fold(f64::NEG_INFINITY, f64::max);
                        let start = (lo.floor() - 2.).clamp(0., f64::from(limit - 8)) as u32;
                        let length = ((hi.ceil() + 2.).min(f64::from(limit)) - f64::from(start))
                            .max(8.) as u32;
                        (start, length.min(limit - start))
                    };
                    let (x, w) = bound(0, size.0);
                    let (y, h) = bound(1, size.1);
                    let begin = image.begin as f64 / 1000.;
                    let end = image.end as f64 / 1000.;
                    let mut effect = String::new();
                    if image.crop.is_some()
                        || image
                            .alpha
                            .iter()
                            .any(|v| (*v - image.alpha[0]).abs() > 0.000001)
                    {
                        let crop = image.crop.clone().unwrap_or(Crop {
                            left: 0.,
                            top: 0.,
                            right: 1.,
                            bottom: 1.,
                        });
                        effect=format!(",geq=r='r(X,Y)':g='g(X,Y)':b='b(X,Y)':a='alpha(X,Y)*({})*gte(X,{:.6}*W)*lt(X,{:.6}*W)*gte(Y,{:.6}*H)*lt(Y,{:.6}*H)'",expression(&image.alpha,0,"N"),crop.left,crop.right,crop.top,crop.bottom);
                    } else if image.alpha[0] < 0.999999 {
                        effect = format!(
                            ",format=rgba,colorchannelmixer=aa={:.6},format=gbrap",
                            image.alpha[0]
                        );
                    }
                    let perspective = image
                        .corners
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            format!(
                                "{}{}='{}'",
                                if i % 2 == 0 { "x" } else { "y" },
                                i / 2,
                                expression(
                                    &v.iter()
                                        .map(|n| n - f64::from(if i % 2 == 0 { x } else { y }))
                                        .collect::<Vec<_>>(),
                                    0,
                                    "on"
                                )
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(":");
                    // Transparent padding prevents perspective's edge extension from smearing image borders.
                    graph.push_str(&format!(";[{input}:v]scale={}:{}:flags=lanczos,format=rgba,pad={w}:{h}:2:2:color=black@0,format=gbrap{effect},perspective=sense=destination:eval=frame:interpolation=cubic:{perspective},setparams=alpha_mode=straight,settb=AVTB,setpts=PTS-STARTPTS+{begin:.6}/TB[mimg{index}];[{base}][mimg{index}]overlay=x={x}:y={y}:format=rgb:alpha=straight:eof_action=pass:repeatlast=0:enable='gte(t,{begin:.6})*lt(t,{end:.6})'[{next}]",w-4,h-4));
                    input += 1;
                }
            }
            base = next;
        }
        Ok((base, files))
    }
}
pub fn preview(
    p: &Project,
    gid: Uuid,
    root: &FsPath,
    directory: &FsPath,
    output: &FsPath,
    size: (u32, u32),
    display: (u32, u32),
) -> anyhow::Result<()> {
    let g = p
        .motion
        .graphics
        .iter()
        .find(|g| g.id == gid)
        .ok_or_else(|| anyhow::anyhow!("Grafica assente"))?;
    let (_, begin, end) = span(p, g).ok_or_else(|| anyhow::anyhow!("Grafica fuori timeline"))?;
    let mut p = crate::compositor::window(p, begin, end);
    p.motion.graphics.retain(|g| g.id == gid);
    p.motion.graphics[0].approved = true;
    p.subtitles.enabled = false;
    p.subtitles.units.clear();
    let plan = plan(&p, size, root)?;
    let fps = crate::social::frame_rate(&p);
    let fps = format!("{}/{}", fps.numerator, fps.denominator);
    let duration = (end - begin) as f64 / 1000.;
    let mut cmd = Command::new("ffmpeg");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.current_dir(directory).args([
        "-nostdin",
        "-v",
        "error",
        "-n",
        "-filter_complex_threads",
        "1",
    ]);
    plan.inputs(&mut cmd, &fps);
    let mut graph = format!(
        "color=c=black@0:s={}x{}:r={fps}:d={duration:.3},format=gbrap[hbase]",
        size.0, size.1
    );
    let token = Uuid::new_v4();
    let (last, files) = plan.filters(
        &mut graph,
        0,
        "hbase".into(),
        size,
        &fps,
        duration,
        true,
        directory,
        token,
    )?;
    graph.push_str(&format!(";[{last}]setparams=alpha_mode=straight,premultiply=inplace=1,scale={}:{}:flags=bicubic,setparams=alpha_mode=premultiplied,unpremultiply=inplace=1,format=yuva420p[video]",display.0,display.1));
    let path = directory.join(format!("hybrid-{token}.filters"));
    std::fs::write(&path, graph)?;
    let result = cmd
        .arg("-/filter_complex")
        .arg(&path)
        .args([
            "-map",
            "[video]",
            "-t",
            &format!("{duration:.3}"),
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-lossless",
            "1",
            "-auto-alt-ref",
            "0",
            "-threads",
            "2",
            "-row-mt",
            "1",
            "-deadline",
            "realtime",
            "-cpu-used",
            "6",
        ])
        .arg(output)
        .output()?;
    anyhow::ensure!(
        result.status.success(),
        "Anteprima ibrida: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    let _ = std::fs::remove_file(path);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constant_and_balanced_lookup_are_bounded() {
        assert_eq!(expression(&[0.5; 1800], 0, "N"), "0.500000");
        let s = expression(&(0..1800).map(|n| n as f64).collect::<Vec<_>>(), 0, "N");
        assert!(s.len() < 80000);
        let mut depth = 0;
        let mut max = 0;
        for c in s.chars() {
            if c == '(' {
                depth += 1;
                max = max.max(depth);
            } else if c == ')' {
                depth -= 1;
            }
        }
        assert!(max <= 13);
    }
}
