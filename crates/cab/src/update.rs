//! `cab update` — download the latest (or pinned) CLI release archive and
//! replace the local `cab` binary (+ UI) install.

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

/// Directory that holds the running `cab` (preferred) or `~/.cab/bin`.
fn resolve_bin_dir() -> Result<PathBuf, String> {
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let name = format!("cab{}", exe_suffix());
        if dir.join(&name).exists()
            || exe
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(&name))
        {
            return Ok(dir.to_path_buf());
        }
    }
    let dir = install_root().join("bin");
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let old_exe = dir.join(format!("cab{}.old", exe_suffix()));
    if old_exe.exists() {
        let _ = fs::remove_file(&old_exe);
    }
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
        .user_agent(format!("cab/{}", current_version()))
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
        .user_agent(format!("cab/{}", current_version()))
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
    let bin_name = format!("cab{}", platform.exe_suffix);
    let direct = extract_root.join(&bin_name);
    if direct.exists() {
        return Ok(extract_root.to_path_buf());
    }
    for entry in fs::read_dir(extract_root).map_err(|e| format!("read extract dir: {e}"))? {
        let entry = entry.map_err(|e| format!("read extract entry: {e}"))?;
        let path = entry.path();
        if path.is_dir() && path.join(&bin_name).exists() {
            return Ok(path);
        }
    }
    Err(format!(
        "archive does not contain {bin_name} (looked under {})",
        extract_root.display()
    ))
}

fn install_file(src: &Path, dst: &Path) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
    }

    // Try direct copy first (works if target binary is not running/locked).
    if fs::copy(src, dst).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = fs::metadata(dst).map(|m| m.permissions()) {
                perms.set_mode(0o755);
                let _ = fs::set_permissions(dst, perms);
            }
        }
        return Ok(());
    }

    // On Windows (and Unix when binary is in use), rename existing binary first.
    // Operating systems allow renaming open/running executables within the same directory.
    let old = dst.with_extension("exe.old");
    if old.exists() {
        let _ = fs::remove_file(&old);
    }

    if dst.exists() {
        if let Err(rename_err) = fs::rename(dst, &old) {
            #[cfg(windows)]
            {
                let status = std::process::Command::new("cmd")
                    .args(["/C", "copy", "/Y"])
                    .arg(src)
                    .arg(dst)
                    .status();
                if status.map(|s| s.success()).unwrap_or(false) {
                    return Ok(());
                }
            }
            return Err(format!(
                "replace {}: rename failed ({rename_err}) and file is locked (stop the service and retry)",
                dst.display()
            ));
        }
    }

    if let Err(e) = fs::copy(src, dst) {
        let _ = fs::rename(&old, dst);
        return Err(format!("copy {} → {}: {e}", src.display(), dst.display()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(mut perms) = fs::metadata(dst).map(|m| m.permissions()) {
            perms.set_mode(0o755);
            let _ = fs::set_permissions(dst, perms);
        }
    }

    let _ = fs::remove_file(&old);
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

/// Windows-only: write a batch script that replaces the binary + UI after the
/// current process exits (since Windows locks the running executable).
#[cfg(windows)]
fn schedule_post_exit_replace(
    bin_dir: &Path,
    bin_name: &str,
    new_src: &Path,
    ui_src: &Path,
    ui_dir: &Path,
    tmp_dir: &Path,
    _version: &str,
) -> Result<(), String> {
    let cab_exe = bin_dir.join(bin_name);
    let cab_ui = ui_dir;
    let staging_bin = tmp_dir.join(bin_name);
    let staging_ui = tmp_dir.join("ui");
    let log_file = tmp_dir.join("cab-update-apply.log");

    // Copy new binary + UI into the staging dir ( survives the current process ).
    fs::copy(new_src, &staging_bin).map_err(|e| format!("stage binary: {e}"))?;
    if ui_src.is_dir() {
        if staging_ui.exists() {
            fs::remove_dir_all(&staging_ui).ok();
        }
        copy_dir_recursive(ui_src, &staging_ui)?;
    }

    let self_pid = std::process::id();

    // Generate a batch script that will replace + restart after exit.
    let script_path = tmp_dir.join("cab-update-apply.cmd");
    let script_content = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         timeout /t 2 /nobreak >nul\r\n\
         schtasks /End /TN CAB\\cab-srv >nul 2>&1\r\n\
         taskkill /F /IM cab.exe /FI \"PID ne {}\" >nul 2>&1\r\n\
         timeout /t 1 /nobreak >nul\r\n\
         copy /Y \"{}\" \"{}\" >\"{}\" 2>&1\r\n\
         if exist \"{}\" (\r\n\
         rmdir /s /q \"{}\" >>\"{}\" 2>&1\r\n\
         xcopy /E /I /Y \"{}\" \"{}\" >>\"{}\" 2>&1\r\n\
         )\r\n\
         schtasks /Run /TN CAB\\cab-srv >nul 2>&1\r\n\
         del /q \"%~f0\" >nul 2>&1\r\n",
        self_pid,
        staging_bin.display(),
        cab_exe.display(),
        log_file.display(),
        staging_ui.display(),
        cab_ui.display(),
        log_file.display(),
        staging_ui.display(),
        cab_ui.display(),
        log_file.display(),
    );
    fs::write(&script_path, script_content).map_err(|e| format!("write apply script: {e}"))?;

    // Spawn the script in a new window ( hidden via wscript ).
    let vbs_path = tmp_dir.join("cab-update-apply.vbs");
    let vbs = format!(
        "Set sh = CreateObject(\"WScript.Shell\")\r\n\
         sh.Run \"cmd /c \"\"{}\"\"\", 0, False\r\n",
        script_path.display()
    );
    fs::write(&vbs_path, vbs).map_err(|e| format!("write vbs: {e}"))?;

    let status = std::process::Command::new("wscript.exe")
        .arg(&vbs_path)
        .status()
        .map_err(|e| format!("spawn apply script: {e}"))?;
    if !status.success() {
        return Err("Failed to spawn post-exit updater".into());
    }
    Ok(())
}

/// Update flow entry point.
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

    // Best-effort stop so other cab.exe processes are gone.
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

    let bin_name = format!("cab{}", platform.exe_suffix);
    let bin_src = payload.join(&bin_name);
    let ui_src = payload.join("ui");

    let target_bin = bin_dir.join(&bin_name);
    let install_res = install_file(&bin_src, &target_bin);

    #[cfg(windows)]
    if install_res.is_err() {
        println!("Direct replacement locked, scheduling background post-exit replace...");
        let ui_exists = ui_src.is_dir();
        schedule_post_exit_replace(
            &bin_dir,
            &bin_name,
            &bin_src,
            if ui_exists { &ui_src } else { Path::new("") },
            &ui_dir,
            &tmp,
            remote,
        )?;
        write_install_meta(remote, platform, &bin_dir, &ui_dir);
        println!("Staged update for CAB {remote}. Service will be restarted automatically.");
        println!("CAB {remote} ready. Run: cab status");
        return Ok(());
    }

    install_res?;

    if ui_src.is_dir() {
        if ui_dir.exists() {
            fs::remove_dir_all(&ui_dir).map_err(|e| format!("clear {}: {e}", ui_dir.display()))?;
        }
        copy_dir_recursive(&ui_src, &ui_dir)?;
        println!("Updated UI → {}", ui_dir.display());
    }

    write_install_meta(remote, platform, &bin_dir, &ui_dir);
    let _ = fs::remove_dir_all(&tmp);

    println!("Updated binary in {}", bin_dir.display());
    match crate::service::start_daemon() {
        Ok(()) => println!("Service restarted."),
        Err(e) => println!("Warning: could not restart service: {e}"),
    }
    println!("CAB {remote} ready. Run: cab status");
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
