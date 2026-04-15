use std::path::PathBuf;

/// The skippr-el version that this build of skippr expects.
/// Updated when skippr is built against a new skippr-el release.
pub const SKIPPR_VERSION: &str = "8.0.0";

const GITHUB_OWNER: &str = "skipprd";
const GITHUB_REPO: &str = "skipprd";

/// Resolves the path to the `skippr-el` binary.
///
/// 1. If `skippr-el` is on PATH, use it (user has an explicit install).
/// 2. If a managed copy exists at `~/.skippr/bin/skippr-el` **and** its
///    version marker matches `SKIPPR_VERSION`, use it.
/// 3. Otherwise, download the pinned version from GitHub releases.
pub async fn resolve_skippr_binary() -> Result<String, String> {
    if which_skippr_el() {
        return Ok("skippr-el".to_string());
    }

    let managed = managed_binary_path()?;
    if managed.is_file() && cached_version_matches() {
        return Ok(managed.to_string_lossy().to_string());
    }

    if managed.is_file() {
        eprintln!(
            "[skippr] upgrading extract-and-load engine to v{}...",
            SKIPPR_VERSION
        );
    } else {
        eprintln!(
            "[skippr] downloading extract-and-load engine v{}...",
            SKIPPR_VERSION
        );
    }
    download_skippr_el(&managed).await?;
    write_version_marker();

    Ok(managed.to_string_lossy().to_string())
}

/// Returns the path where we store the managed skippr-el binary.
pub fn managed_binary_path() -> Result<PathBuf, String> {
    let home = dirs_next::home_dir().ok_or("could not determine home directory")?;
    let bin_dir = home.join(".skippr").join("bin");
    Ok(bin_dir.join(skippr_el_binary_name()))
}

fn version_marker_path() -> Option<PathBuf> {
    let home = dirs_next::home_dir()?;
    Some(home.join(".skippr").join("bin").join(".skippr-el-version"))
}

fn cached_version_matches() -> bool {
    version_marker_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|v| v.trim() == SKIPPR_VERSION)
        .unwrap_or(false)
}

fn write_version_marker() {
    if let Some(marker) = version_marker_path() {
        let _ = std::fs::write(marker, SKIPPR_VERSION);
    }
}

fn skippr_el_binary_name() -> &'static str {
    if cfg!(windows) {
        "skippr-el.exe"
    } else {
        "skippr-el"
    }
}

fn which_skippr_el() -> bool {
    let cmd = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(cmd)
        .arg("skippr-el")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn asset_pattern() -> Result<&'static str, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => Ok("macos_arm64.tar.gz"),
        ("macos", "x86_64") => Ok("macos_x86.tar.gz"),
        ("linux", "aarch64") => Ok("linux_arm64.tar.gz"),
        ("linux", "x86_64") => Ok("linux_x86.tar.gz"),
        ("windows", "x86_64") => Ok("windows_x86.tar.gz"),
        _ => Err(format!(
            "unsupported platform: os={} arch={} — download skippr-el manually and place it on PATH",
            os, arch
        )),
    }
}

async fn download_skippr_el(dest: &PathBuf) -> Result<(), String> {
    let pattern = asset_pattern()?;
    let release_url = format!(
        "https://api.github.com/repos/{}/{}/releases/tags/{}",
        GITHUB_OWNER, GITHUB_REPO, SKIPPR_VERSION
    );

    let client = reqwest::Client::new();

    let release: serde_json::Value = client
        .get(&release_url)
        .header("User-Agent", "skippr")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("failed to fetch release metadata: {}", e))?
        .json()
        .await
        .map_err(|e| format!("failed to parse release metadata: {}", e))?;

    let assets = release["assets"]
        .as_array()
        .ok_or("no assets found in release")?;

    let download_url = assets
        .iter()
        .filter_map(|a| {
            let name = a["name"].as_str()?;
            if name.contains(pattern) || name.ends_with(pattern) {
                a["browser_download_url"].as_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| {
            format!(
                "no release asset matching '{}' found for skippr-el v{}",
                pattern, SKIPPR_VERSION
            )
        })?;

    eprintln!("[skippr] downloading {}...", download_url);

    let bytes = client
        .get(&download_url)
        .header("User-Agent", "skippr")
        .send()
        .await
        .map_err(|e| format!("download failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("failed to read download: {}", e))?;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create {}: {}", parent.display(), e))?;
    }

    extract_skippr_el_from_tarball(&bytes, dest)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to set executable permission: {}", e))?;
    }

    eprintln!(
        "[skippr] installed skippr-el v{} to {}",
        SKIPPR_VERSION,
        dest.display()
    );
    Ok(())
}

fn extract_skippr_el_from_tarball(tarball: &[u8], dest: &PathBuf) -> Result<(), String> {
    use std::io::Read;

    let gz = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(gz);

    let target_name = skippr_el_binary_name();

    for entry in archive
        .entries()
        .map_err(|e| format!("failed to read tar entries: {}", e))?
    {
        let mut entry = entry.map_err(|e| format!("failed to read tar entry: {}", e))?;
        let path = entry
            .path()
            .map_err(|e| format!("failed to read entry path: {}", e))?;

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == target_name {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("failed to read skippr-el binary from archive: {}", e))?;
            std::fs::write(dest, &buf)
                .map_err(|e| format!("failed to write {}: {}", dest.display(), e))?;
            return Ok(());
        }
    }

    Err(format!(
        "'{}' not found in the downloaded archive",
        target_name
    ))
}
