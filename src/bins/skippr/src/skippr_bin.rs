use std::path::PathBuf;

/// The skippr-el version that this build of skippr expects.
/// Updated when skippr is built against a new skippr-el release.
pub const SKIPPR_VERSION: &str = "8.1.0";

const RELEASES_BASE_URL: &str = "https://install.skippr.io/releases";
const SKIPPR_EL_RELEASE_SUBDIR: &str = "skippr-el";

/// Resolves the path to the `skippr-el` binary.
///
/// 1. If `skippr-el` is on PATH, use it (user has an explicit install).
/// 2. If a managed copy exists at `~/.skippr/bin/skippr-el` **and** its
///    version marker matches `SKIPPR_VERSION`, use it.
/// 3. Otherwise, download the pinned version from the install bucket.
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

fn asset_filename() -> Result<&'static str, String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => Ok("skippr-el-macos_arm64.tar.gz"),
        ("macos", "x86_64") => Ok("skippr-el-macos_x86.tar.gz"),
        ("linux", "aarch64") => Ok("skippr-el-linux_arm64.tar.gz"),
        ("linux", "x86_64") => Ok("skippr-el-linux_x86.tar.gz"),
        ("windows", "x86_64") => Ok("skippr-el-windows_x86.tar.gz"),
        _ => Err(format!(
            "unsupported platform: os={} arch={} — download skippr-el manually and place it on PATH",
            os, arch
        )),
    }
}

fn skippr_el_release_url() -> Result<String, String> {
    let filename = asset_filename()?;
    Ok(format!(
        "{}/{}/{}/{}",
        RELEASES_BASE_URL, SKIPPR_EL_RELEASE_SUBDIR, SKIPPR_VERSION, filename
    ))
}

async fn download_skippr_el(dest: &PathBuf) -> Result<(), String> {
    let download_url = skippr_el_release_url()?;
    let client = reqwest::Client::new();

    eprintln!("[skippr] downloading {}...", download_url);

    let response = client
        .get(&download_url)
        .header("User-Agent", "skippr")
        .send()
        .await
        .map_err(|e| format!("download failed: {}", e))?;
    if !response.status().is_success() {
        return Err(format!(
            "download returned status {} from {}",
            response.status(),
            download_url
        ));
    }
    let bytes = response
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

#[cfg(test)]
mod tests {
    use super::{asset_filename, skippr_el_release_url, SKIPPR_VERSION};

    #[test]
    fn asset_filename_matches_current_platform() {
        let expected = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "skippr-el-macos_arm64.tar.gz",
            ("macos", "x86_64") => "skippr-el-macos_x86.tar.gz",
            ("linux", "aarch64") => "skippr-el-linux_arm64.tar.gz",
            ("linux", "x86_64") => "skippr-el-linux_x86.tar.gz",
            ("windows", "x86_64") => "skippr-el-windows_x86.tar.gz",
            _ => return,
        };

        assert_eq!(asset_filename().unwrap(), expected);
    }

    #[test]
    fn release_url_uses_product_first_prefix() {
        let filename = asset_filename().unwrap();
        assert_eq!(
            skippr_el_release_url().unwrap(),
            format!(
                "https://install.skippr.io/releases/skippr-el/{}/{}",
                SKIPPR_VERSION, filename
            )
        );
    }
}
