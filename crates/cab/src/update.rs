//! `cab-cli update` — download the latest (or pinned) CLI release archive and
//! replace the local `cab-cli` / `cab-srv` (+ UI) install.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const DEFAULT_REPO: &str = "xiongdi/cab";

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Copy)]
struct Platform {
    os: &'static str,
    arch: &'static str,
    archive_ext: &'static str,
    exe_suffix: &'static str,
}

fn detect_platform() -> Result<Platform, String> {
    let os = match env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => return Err(format!("Unsupported OS for update: {other}")),
    };
    let arch = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => return Err(format!("Unsupported arch for update: {other}")),
    };
    let (archive_ext, exe_suffix) = if os == "windows" {
        ("zip", ".exe")
    } else {
        ("tar.gz", "")
    };
    Ok(Platform {
        os,
        arch,
        archive_ext,
        exe_suffix,
    })
}

fn asset_name(p: Platform) -> String {
    format!("cab-{}-{}.{}", p.os, p.arch, p.archive_ext)
}

fn repo() -> String {
    env::var("CAB_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

fn install_root() -> PathBuf {
    if let Ok(root) = env::var("CAB_INSTALL_ROOT") {
        return PathBuf::from(root);
    }
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".cab")
}

/// Directory that holds the running `cab-cli` (preferred) or `~/.cab/bin`.
fn resolve_bin_dir() -> Result<PathBuf, String> {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let srv = dir.join(format!("cab-srv{}", exe_suffix()));
        if srv.exists() || dir.join("cab-cli").exists() || dir.join("cab-cli.exe").exists() {
            return Ok(dir.to_path_buf());
        }
    }
    let dir = install_root().join("bin");
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    Ok(dir)
}

fn resolve_ui_dir(bin_dir: &Path) -> PathBuf {
    // Preferred curl-install layout: ~/.cab/bin + ~/.cab/ui
    let sibling = bin_dir
        .parent()
        .map(|p| p.join("ui"))
        .unwrap_or_else(|| install_root().join("ui"));
    if sibling.exists() || bin_dir.ends_with("bin") {
        return sibling;
    }
    bin_dir.join("ui")
}

fn exe_suffix() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

fn strip_v(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

async fn fetch_release(tag: Option<&str>) -> Result<GhRelease, String> {
    let repo = repo();
    let url = match tag {
        Some(t) => {
            let t = if t.starts_with('v') {
                t.to_string()
            } else {
                format!("v{t}")
            };
            format!("https://api.github.com/repos/{repo}/releases/tags/{t}")
        }
        None => format!("https://api.github.com/repos/{repo}/releases/latest"),
    };
    let client = reqwest::Client::builder()
        .user_agent(format!("cab-cli/{}", current_version()))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "GitHub API {} → HTTP {}",
            url,
            resp.status().as_u16()
        ));
    }
    resp.json::<GhRelease>()
        .await
        .map_err(|e| format!("parse GitHub release JSON: {e}"))
}

async fn download_to(url: &str, dest: &Path) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent(format!("cab-cli/{}", current_version()))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "download {} → HTTP {}",
            url,
            resp.status().as_u16()
        ));
    }
    let bytes = resp.bytes().await.map_err(|e| format!("read body: {e}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let mut f = fs::File::create(dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    f.write_all(&bytes)
        .map_err(|e| format!("write {}: {e}", dest.display()))?;
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path, platform: Platform) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| format!("mkdir extract: {e}"))?;
    if platform.archive_ext == "tar.gz" {
        let status = Command::new("tar")
            .args(["-xzf"])
            .arg(archive)
            .arg("-C")
            .arg(dest)
            .status()
            .map_err(|e| format!("failed to run tar: {e}"))?;
        if !status.success() {
            return Err(format!("tar exited with {status}"));
        }
    } else {
        // Prefer unzip; fall back to PowerShell Expand-Archive on Windows.
        let unzip = Command::new("unzip")
            .args(["-q", "-o"])
            .arg(archive)
            .arg("-d")
            .arg(dest)
            .status();
        match unzip {
            Ok(s) if s.success() => {}
            _ => {
                let status = Command::new("powershell")
                    .args([
                        "-NoProfile",
                        "-NonInteractive",
                        "-Command",
                        &format!(
                            "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
                            archive.display(),
                            dest.display()
                        ),
                    ])
                    .status()
                    .map_err(|e| format!("failed to expand zip: {e}"))?;
                if !status.success() {
                    return Err(format!("zip extract failed: {status}"));
                }
            }
        }
    }
    Ok(())
}

fn find_payload(extract_root: &Path, platform: Platform) -> Result<PathBuf, String> {
    let cli_name = format!("cab-cli{}", platform.exe_suffix);
    let direct = extract_root.join(&cli_name);
    if direct.exists() {
        return Ok(extract_root.to_path_buf());
    }
    for entry in fs::read_dir(extract_root).map_err(|e| format!("read extract dir: {e}"))? {
        let entry = entry.map_err(|e| format!("read extract entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() && path.join(&cli_name).exists() {
            return Ok(path);
        }
    }
    Err(format!(
        "archive does not contain {cli_name} (looked under {})",
        extract_root.display()
    ))
}

fn install_file(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }
    // Write to a temp sibling then rename — safer while the binary may be running.
    let tmp = dst.with_extension("updating");
    fs::copy(src, &tmp).map_err(|e| format!("copy {} → {}: {e}", src.display(), tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&tmp)
            .map_err(|e| format!("stat {}: {e}", tmp.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp, perms).map_err(|e| format!("chmod {}: {e}", tmp.display()))?;
    }
    fs::rename(&tmp, dst).or_else(|_| {
        // Windows may refuse rename over a running exe — try direct copy.
        fs::copy(src, dst).map(|_| ()).map_err(|e| {
            format!(
                "replace {}: {e} (stop the service and retry if the file is locked)",
                dst.display()
            )
        })
    })?;
    Ok(())
}

fn write_install_meta(version: &str, platform: Platform, bin_dir: &Path, ui_dir: &Path) {
    let root = install_root();
    let _ = fs::create_dir_all(&root);
    let meta = format!(
        "{{\n  \"version\": \"{version}\",\n  \"os\": \"{}\",\n  \"arch\": \"{}\",\n  \"bin_dir\": \"{}\",\n  \"ui_dir\": \"{}\"\n}}\n",
        platform.os,
        platform.arch,
        bin_dir.display(),
        ui_dir.display()
    );
    let _ = fs::write(root.join("install.json"), meta);
}

/// Check / download / apply a CLI release update.
pub async fn run_update(check_only: bool, version: Option<String>) -> Result<(), String> {
    let platform = detect_platform()?;
    let wanted = asset_name(platform);
    let release = fetch_release(version.as_deref()).await?;
    let remote = strip_v(&release.tag_name);
    let local = current_version();

    println!("Installed: {local}");
    println!("Available: {remote} ({})", release.tag_name);

    if remote == local && version.is_none() {
        println!("Already up to date.");
        return Ok(());
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == wanted)
        .ok_or_else(|| {
            format!(
                "Release {} has no asset `{wanted}`. \
                 Publish CLI archives via scripts/package-cli.sh / CI.",
                release.tag_name
            )
        })?;

    if check_only {
        println!("Update available: {} → {remote}", local);
        println!("Download: {}", asset.browser_download_url);
        return Ok(());
    }

    let bin_dir = resolve_bin_dir()?;
    let ui_dir = resolve_ui_dir(&bin_dir);
    println!("Install dir: {}", bin_dir.display());

    // Best-effort stop so binaries can be replaced.
    let _ = crate::service::stop_daemon();

    let tmp = env::temp_dir().join(format!("cab-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| format!("tmpdir: {e}"))?;
    let archive = tmp.join(&wanted);
    println!("Downloading {}…", asset.name);
    download_to(&asset.browser_download_url, &archive).await?;

    let extract = tmp.join("extract");
    extract_archive(&archive, &extract, platform)?;
    let payload = find_payload(&extract, platform)?;

    let cli_name = format!("cab-cli{}", platform.exe_suffix);
    let srv_name = format!("cab-srv{}", platform.exe_suffix);
    install_file(&payload.join(&cli_name), &bin_dir.join(&cli_name))?;
    install_file(&payload.join(&srv_name), &bin_dir.join(&srv_name))?;

    let ui_src = payload.join("ui");
    if ui_src.is_dir() {
        if ui_dir.exists() {
            fs::remove_dir_all(&ui_dir).map_err(|e| format!("clear {}: {e}", ui_dir.display()))?;
        }
        copy_dir_recursive(&ui_src, &ui_dir)?;
        println!("Updated UI → {}", ui_dir.display());
    }

    write_install_meta(remote, platform, &bin_dir, &ui_dir);
    let _ = fs::remove_dir_all(&tmp);

    println!("Updated binaries in {}", bin_dir.display());
    match crate::service::start_daemon() {
        Ok(()) => println!("Service restarted."),
        Err(e) => println!("Warning: could not restart service: {e}"),
    }
    println!("CAB {remote} ready. Run: cab-cli status");
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("mkdir {}: {e}", dst.display()))?;
    for entry in fs::read_dir(src).map_err(|e| format!("read {}: {e}", src.display()))? {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)
                .map_err(|e| format!("copy {} → {}: {e}", from.display(), to.display()))?;
        }
    }
    Ok(())
}
