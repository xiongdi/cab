use axum::Json;
use axum::response::IntoResponse;
use cab_core::CabError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct CheckUpdateResponse {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: Option<String>,
    pub published_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallUpdateResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

fn is_newer(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = current
        .trim_start_matches('v')
        .split('.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();
    let latest_parts: Vec<u32> = latest
        .trim_start_matches('v')
        .split('.')
        .map(|s| s.parse().unwrap_or(0))
        .collect();

    for i in 0..3 {
        let curr = current_parts.get(i).copied().unwrap_or(0);
        let lat = latest_parts.get(i).copied().unwrap_or(0);
        if lat > curr {
            return true;
        } else if lat < curr {
            return false;
        }
    }
    false
}

fn select_asset(assets: &[GithubAsset]) -> Option<&GithubAsset> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    for asset in assets {
        let name = asset.name.to_lowercase();
        match os {
            "windows"
                if (name.ends_with(".msi") || name.ends_with(".exe"))
                    && ((arch == "aarch64" && name.contains("arm64"))
                        || (arch == "x86_64"
                            && (name.contains("x64")
                                || name.contains("amd64")
                                || name.contains("setup")))) =>
            {
                return Some(asset);
            }
            "macos" => {
                if name.ends_with(".dmg") || name.contains("universal") {
                    return Some(asset);
                }
            }
            "linux"
                if (name.ends_with(".deb") || name.ends_with(".appimage"))
                    && ((arch == "aarch64" && name.contains("arm64"))
                        || (arch == "x86_64"
                            && (name.contains("amd64") || name.contains("x86_64")))) =>
            {
                return Some(asset);
            }
            _ => {}
        }
    }

    // Fallback: look for matching OS without strict architecture match
    for asset in assets {
        let name = asset.name.to_lowercase();
        match os {
            "windows" if name.ends_with(".msi") || name.ends_with(".exe") => {
                return Some(asset);
            }
            "macos" => {
                if name.ends_with(".dmg") || name.ends_with(".tar.gz") {
                    return Some(asset);
                }
            }
            "linux"
                if name.ends_with(".deb")
                    || name.ends_with(".appimage")
                    || name.ends_with(".rpm") =>
            {
                return Some(asset);
            }
            _ => {}
        }
    }

    None
}

fn open_installer(file_path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(&["/c", "start", "", &file_path.to_string_lossy()])
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(file_path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if extension.to_lowercase() == "appimage" {
            let _ = std::process::Command::new("chmod")
                .arg("+x")
                .arg(file_path)
                .status();
        }
        std::process::Command::new("xdg-open")
            .arg(file_path)
            .spawn()?;
    }
    Ok(())
}

pub async fn check_update() -> Result<impl IntoResponse, CabError> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let client = reqwest::Client::builder()
        .user_agent("CAB-Updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| CabError::Proxy(format!("Failed to build HTTP client: {e}")))?;

    let response = client
        .get("https://api.github.com/repos/xiongdi/cab/releases/latest")
        .send()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to contact GitHub: {e}")))?;

    if !response.status().is_success() {
        return Err(CabError::Proxy(format!(
            "GitHub returned status {}",
            response.status()
        )));
    }

    let release: GithubRelease = response
        .json()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to parse GitHub response: {e}")))?;

    let latest_version = release.tag_name.clone();
    let available = is_newer(&current_version, &latest_version);

    let download_url = select_asset(&release.assets).map(|a| a.browser_download_url.clone());

    Ok(Json(CheckUpdateResponse {
        available,
        current_version,
        latest_version,
        release_notes: release.body.unwrap_or_default(),
        download_url,
        published_at: release.published_at,
    }))
}

pub async fn install_update() -> Result<impl IntoResponse, CabError> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    let client = reqwest::Client::builder()
        .user_agent("CAB-Updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| CabError::Proxy(format!("Failed to build HTTP client: {e}")))?;

    let response = client
        .get("https://api.github.com/repos/xiongdi/cab/releases/latest")
        .send()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to contact GitHub: {e}")))?;

    if !response.status().is_success() {
        return Err(CabError::Proxy(format!(
            "GitHub returned status {}",
            response.status()
        )));
    }

    let release: GithubRelease = response
        .json()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to parse GitHub response: {e}")))?;

    let latest_version = release.tag_name.clone();
    if !is_newer(&current_version, &latest_version) {
        return Ok(Json(InstallUpdateResponse {
            success: false,
            message: "No update available".to_string(),
        }));
    }

    let asset = match select_asset(&release.assets) {
        Some(a) => a,
        None => {
            return Err(CabError::NotFound(format!(
                "No suitable installer found for your platform (OS: {}, Arch: {})",
                std::env::consts::OS,
                std::env::consts::ARCH
            )));
        }
    };

    let download_url = &asset.browser_download_url;
    let download_resp = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to download update: {e}")))?;

    if !download_resp.status().is_success() {
        return Err(CabError::Proxy(format!(
            "Failed to download update, status: {}",
            download_resp.status()
        )));
    }

    let bytes = download_resp
        .bytes()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to read update bytes: {e}")))?;

    let temp_dir = std::env::temp_dir();
    let file_path = temp_dir.join(&asset.name);

    std::fs::write(&file_path, bytes)
        .map_err(|e| CabError::Database(format!("Failed to write installer file: {e}")))?;

    if let Err(e) = open_installer(&file_path) {
        return Err(CabError::Config(format!("Failed to launch installer: {e}")));
    }

    Ok(Json(InstallUpdateResponse {
        success: true,
        message: format!(
            "Successfully downloaded and launched installer: {}",
            asset.name
        ),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.5.0", "0.5.1"));
        assert!(is_newer("v0.5.0", "v0.6.0"));
        assert!(is_newer("0.5.0", "1.0.0"));
        assert!(!is_newer("0.5.0", "0.5.0"));
        assert!(!is_newer("0.5.1", "0.5.0"));
        assert!(!is_newer("1.0.0", "0.5.0"));
    }

    #[test]
    fn test_select_asset() {
        let assets = vec![
            GithubAsset {
                name: "CAB_0.5.0_x64_zh-CN.msi".to_string(),
                browser_download_url: "https://example.com/win_x64".to_string(),
            },
            GithubAsset {
                name: "CAB_0.5.0_universal.dmg".to_string(),
                browser_download_url: "https://example.com/mac".to_string(),
            },
            GithubAsset {
                name: "CAB_0.5.0_amd64.deb".to_string(),
                browser_download_url: "https://example.com/linux_x64".to_string(),
            },
        ];

        let selected = select_asset(&assets);
        assert!(selected.is_some());
    }
}
