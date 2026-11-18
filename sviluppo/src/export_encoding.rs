//! Encoder controls for the existing compositor, not a second render pipeline.
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Encoding {
    pub device: String,
    pub speed: String,
    pub rate_mode: String,
    pub hardware_cq: u8,
    pub bitrate_kbps: u32,
    pub allow_cpu_fallback: bool,
}
impl Default for Encoding {
    fn default() -> Self {
        Self {
            device: "cpu".into(),
            speed: "fast".into(),
            rate_mode: "quality".into(),
            hardware_cq: 21,
            bitrate_kbps: 12000,
            allow_cpu_fallback: false,
        }
    }
}
pub fn encoder(codec: &str, device: &str) -> anyhow::Result<&'static str> {
    Ok(match (codec, device) {
        ("h264", "cpu") => "libx264",
        ("hevc", "cpu") => "libx265",
        ("av1", "cpu") => "libsvtav1",
        ("h264", "nvenc") => "h264_nvenc",
        ("hevc", "nvenc") => "hevc_nvenc",
        ("av1", "nvenc") => "av1_nvenc",
        _ => anyhow::bail!("Codec/dispositivo non supportato"),
    })
}
impl Encoding {
    pub fn validate(&self, codec: &str) -> anyhow::Result<()> {
        encoder(codec, &self.device)?;
        ensure!(
            ["ultrafast", "fast", "medium", "slow"].contains(&self.speed.as_str()),
            "Velocità encoder non valida"
        );
        ensure!(
            ["quality", "bitrate"].contains(&self.rate_mode.as_str())
                && (1..=51).contains(&self.hardware_cq)
                && (100..=200000).contains(&self.bitrate_kbps),
            "CQ / bitrate fuori intervallo"
        );
        Ok(())
    }
    pub fn args(&self, codec: &str, crf: u8) -> anyhow::Result<Vec<String>> {
        self.validate(codec)?;
        let enc = encoder(codec, &self.device)?;
        let speed = match (codec, self.device.as_str(), self.speed.as_str()) {
            (_, "nvenc", "ultrafast") => "p1",
            (_, "nvenc", "fast") => "p3",
            (_, "nvenc", "medium") => "p5",
            (_, "nvenc", _) => "p7",
            ("av1", _, "ultrafast") => "12",
            ("av1", _, "fast") => "10",
            ("av1", _, "medium") => "8",
            ("av1", _, _) => "6",
            _ => &self.speed,
        };
        let mut args = vec![
            "-c:v".into(),
            enc.into(),
            "-threads".into(),
            "2".into(),
            "-preset".into(),
            speed.into(),
        ];
        if codec == "av1" && self.device == "cpu" {
            args.extend(["-svtav1-params".into(), "lp=2".into()]);
        }
        if codec == "hevc" && self.device == "cpu" {
            args.extend(["-x265-params".into(), "pools=2:frame-threads=2".into()]);
        }
        if self.rate_mode == "bitrate" {
            args.extend(["-b:v".into(), format!("{}k", self.bitrate_kbps)]);
        } else if self.device == "nvenc" {
            args.extend([
                "-rc".into(),
                "vbr".into(),
                "-cq".into(),
                self.hardware_cq.to_string(),
                "-b:v".into(),
                "0".into(),
            ]);
        } else {
            args.extend(["-crf".into(), crf.to_string()]);
        }
        if codec == "hevc" {
            args.extend(["-tag:v".into(), "hvc1".into()]);
        }
        Ok(args)
    }
}
fn listed() -> anyhow::Result<String> {
    let result = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .context("FFmpeg assente")?;
    ensure!(
        result.status.success(),
        "Impossibile interrogare gli encoder"
    );
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}
pub async fn capabilities() -> axum::Json<Value> {
    let list = tokio::task::spawn_blocking(listed)
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    let encoders=["h264","hevc","av1"].iter().flat_map(|codec|["cpu","nvenc"].map(|device|{let name=encoder(codec,device).unwrap();json!({"codec":codec,"device":device,"encoder":name,"compiled":list.split_whitespace().any(|s|s==name),"runtime_verified":false})})).collect::<Vec<_>>();
    axum::Json(
        json!({"encoders":encoders,"note":"Presenza nel binario, non prova del driver. Un encode di verifica viene eseguito soltanto quando richiedi l'export; nessun unload o modifica dei servizi."}),
    )
}
/// One tiny encode BEFORE the real render. No partial output/re-encoding on failure.
pub fn resolve(
    codec: &str,
    encoding: &Encoding,
    crf: u8,
) -> anyhow::Result<(Encoding, Option<String>)> {
    let args = encoding.args(codec, crf)?;
    let probe = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=s=256x256:r=30:d=0.04",
        ])
        .args(args)
        .args(["-frames:v", "1", "-f", "null", "-"])
        .output()?;
    if probe.status.success() {
        return Ok((encoding.clone(), None));
    }
    let reason = String::from_utf8_lossy(&probe.stderr);
    ensure!(encoding.device != "cpu" && encoding.allow_cpu_fallback, "Encoder richiesto non operativo: {reason}. Nessun fallback autorizzato, nessun render avviato.");
    let cpu = Encoding {
        device: "cpu".into(),
        allow_cpu_fallback: false,
        ..encoding.clone()
    };
    let (effective, _) = resolve(codec, &cpu, crf)?;
    Ok((
        effective,
        Some(format!("Fallback CPU autorizzato: {reason}")),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoder_controls_are_not_shared_quality_numbers() {
        let mut e = Encoding::default();
        assert!(e.args("h264", 18).unwrap().contains(&"-crf".into()));
        e.device = "nvenc".into();
        let args = e.args("h264", 18).unwrap();
        assert!(args.contains(&"-cq".into()));
        assert!(!args.contains(&"-crf".into()));
        assert!(args.contains(&"21".into()));
        e.speed = "fast;movie=private".into();
        assert!(e.args("h264", 18).is_err());
    }
}
