//! Read-only installed-font catalog. Never installs, copies or redistributes fonts.
use axum::Json;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Serialize)]
pub struct Catalog {
    pub families: Vec<String>,
    pub warning: Option<String>,
    pub fallback: String,
    pub redistribution: &'static str,
}
static CATALOG: OnceLock<Catalog> = OnceLock::new();
pub fn safe_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.len() <= 120
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || " -_./".contains(c))
}
fn scan() -> Catalog {
    let mut families = vec![
        "Arial".into(),
        "Arial Black".into(),
        "Segoe UI".into(),
        "Tahoma".into(),
    ];
    let mut warning = None;
    #[cfg(windows)]
    {
        use std::{os::windows::process::CommandExt, process::Command};
        // Constant command; no user input or font name is interpolated into a shell.
        let script = r#"$ErrorActionPreference='Stop'; [Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false); Add-Type -AssemblyName PresentationCore; $f=@([System.Windows.Media.Fonts]::SystemFontFamilies | ForEach-Object { $_.Source }); $d=Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Microsoft\Windows\Fonts'; if(Test-Path -LiteralPath $d){$f+=@([System.Windows.Media.Fonts]::GetFontFamilies($d+[IO.Path]::DirectorySeparatorChar) | ForEach-Object { $_.FamilyNames.Values })}; ConvertTo-Json -Compress -InputObject @($f | Sort-Object -Unique)"#;
        let powershell = std::env::var_os("SystemRoot")
            .map(std::path::PathBuf::from)
            .map(|p| p.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| "powershell.exe".into());
        match Command::new(powershell)
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
        {
            Ok(output) if output.status.success() && output.stdout.len() < 512 * 1024 => {
                match serde_json::from_slice::<Vec<String>>(&output.stdout) {
                    Ok(names) => families.extend(names.into_iter().filter(|n| safe_name(n))),
                    Err(_) => {
                        warning =
                            Some("Catalogo font non leggibile: disponibili solo i font base".into())
                    }
                }
            }
            _ => {
                warning =
                    Some("Lettura font Windows non disponibile; verifica i font nel render".into())
            }
        }
    }
    #[cfg(not(windows))]
    {
        warning = Some(
            "Catalogo automatico disponibile su Windows; verificare i font sul sistema".into(),
        );
    }
    families.sort_by_key(|a| a.to_lowercase());
    families.dedup();
    Catalog { families, warning, fallback: "Arial".into(), redistribution: "Font del PC, non inclusi nel portable. Verifica separatamente le licenze d'uso e distribuzione." }
}
pub fn catalog() -> &'static Catalog {
    CATALOG.get_or_init(scan)
}
pub fn available(name: &str) -> bool {
    catalog()
        .families
        .iter()
        .any(|f| f.eq_ignore_ascii_case(name))
}
pub fn resolve(name: &str) -> &str {
    if available(name) {
        name
    } else {
        "Arial"
    }
}
pub async fn view() -> Result<Json<&'static Catalog>, crate::ApiError> {
    tokio::task::spawn_blocking(catalog)
        .await
        .map(Json)
        .map_err(|e| crate::ApiError::internal(e.into()))
}
/// libass needs this read-only directory for per-user OTFs on Windows/fontconfig.
/// Same filter option for preview and delivery. No font files are copied.
pub fn directory_option() -> String {
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        let path = std::path::PathBuf::from(root).join("Microsoft/Windows/Fonts");
        let value = path.to_string_lossy().replace('\\', "/");
        if path.is_dir() && !value.contains(['\'', '\n', '\r']) {
            return format!(":fontsdir='{}'", value.replace(':', "\\:"));
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn font_names_cannot_inject_ass_or_filters() {
        assert!(safe_name("LEMON MILK"));
        assert!(safe_name("Lemon/Milk"));
        for n in ["Arial\\p1", "{\\fnArial}", "font,foo", "a\nb", ""] {
            assert!(!safe_name(n));
        }
    }
}
