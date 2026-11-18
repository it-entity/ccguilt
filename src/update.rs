use anyhow::{anyhow, Result};
use colored::Colorize;
use std::fs;
use std::process::Command;

/// Self-hosted Gitea (LAN only) — web root of the repo and its releases API.
const GITEA_REPO_URL: &str = "http://git-aeoru.local:3000/entity/ccguilt";
const GITEA_API: &str = "http://git-aeoru.local:3000/api/v1/repos/entity/ccguilt/releases";
/// Public GitHub mirror — web root of the repo and its latest-release API.
const GITHUB_REPO_URL: &str = "https://github.com/aayushh-code/ccguilt";
const GITHUB_API: &str = "https://api.github.com/repos/aayushh-code/ccguilt/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Map Rust arch constants to the asset naming conventions used in releases
fn platform_asset_name() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        _ => "linux",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        other => other,
    };
    format!("ccguilt-{}-{}", os, arch)
}

/// Map Rust arch constants to Gitea asset names (which use amd64 convention)
fn gitea_asset_name() -> String {
    let arch = match std::env::consts::ARCH {
        "x86_64" | "amd64" => "amd64",
        "aarch64" | "arm64" => "aarch64",
        other => other,
    };
    format!("ccguilt-linux-{}", arch)
}

pub fn self_update() -> Result<()> {
    eprintln!(
        "  {} Checking for updates (current: v{})...",
        ">>".yellow().bold(),
        CURRENT_VERSION
    );

    // Try GitHub first (public, works from anywhere), fall back to Gitea (LAN only)
    let (tag, download_url) = match try_github() {
        Ok(result) => result,
        Err(gh_err) => {
            eprintln!(
                "  {} GitHub unavailable ({}), trying Gitea...",
                ">>".yellow().bold(),
                gh_err
            );
            try_gitea()?
        }
    };

    let latest_version = tag.strip_prefix('v').unwrap_or(&tag);

    if latest_version == CURRENT_VERSION {
        eprintln!(
            "  {} Already on latest version (v{}).",
            ">>".green().bold(),
            CURRENT_VERSION
        );
        return Ok(());
    }

    eprintln!(
        "  {} New version available: v{} -> v{}",
        ">>".yellow().bold(),
        CURRENT_VERSION,
        latest_version
    );
    eprintln!("  {} Downloading {}...", ">>".yellow().bold(), download_url);

    // Download to temp file
    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("update");

    let dl = Command::new("curl")
        .args(["-s", "-L", "-o", &tmp_path.to_string_lossy(), &download_url])
        .status()
        .map_err(|_| anyhow!("Download failed."))?;

    if !dl.success() {
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow!("Download failed."));
    }

    // Verify it's not an error page
    let meta = fs::metadata(&tmp_path)?;
    if meta.len() < 1024 {
        let _ = fs::remove_file(&tmp_path);
        return Err(anyhow!(
            "Downloaded file too small ({}B) — release asset may not exist yet.",
            meta.len()
        ));
    }

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))?;
    }

    // Replace current binary
    let backup_path = current_exe.with_extension("old");
    let _ = fs::remove_file(&backup_path);
    fs::rename(&current_exe, &backup_path)?;
    fs::rename(&tmp_path, &current_exe)?;
    let _ = fs::remove_file(&backup_path);

    eprintln!(
        "  {} Updated to v{}! The planet weeps louder.",
        ">>".green().bold(),
        latest_version
    );

    Ok(())
}

fn try_github() -> Result<(String, String)> {
    let output = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "5", GITHUB_API])
        .output()
        .map_err(|_| anyhow!("curl not found"))?;

    if !output.status.success() {
        return Err(anyhow!("GitHub API request failed"));
    }

    let release: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| anyhow!("Bad JSON from GitHub"))?;

    let tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("No tag_name in GitHub release"))?
        .to_string();

    let asset_name = platform_asset_name();
    let download_url = format!(
        "{}/releases/download/{}/{}",
        GITHUB_REPO_URL, tag, asset_name
    );

    Ok((tag, download_url))
}

fn try_gitea() -> Result<(String, String)> {
    let output = Command::new("curl")
        .args(["-s", "-L", "--connect-timeout", "5", GITEA_API])
        .output()
        .map_err(|_| anyhow!("curl not found"))?;

    if !output.status.success() {
        return Err(anyhow!("Failed to reach Gitea update server."));
    }

    let releases: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).map_err(|_| anyhow!("Bad JSON from Gitea"))?;

    let latest = releases
        .first()
        .ok_or_else(|| anyhow!("No releases found on Gitea."))?;

    let tag = latest["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("Invalid release format."))?
        .to_string();

    // Gitea uses amd64 naming convention; build the URL from GITEA_REPO_URL rather than
    // trusting browser_download_url, which follows the server's configured ROOT_URL
    let expected = gitea_asset_name();
    let download_url = latest["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str()?;
                if name == expected {
                    Some(format!(
                        "{}/releases/download/{}/{}",
                        GITEA_REPO_URL, tag, name
                    ))
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| anyhow!("No binary '{}' in Gitea release {}.", expected, tag))?;

    Ok((tag, download_url))
}
