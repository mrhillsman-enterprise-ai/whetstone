use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::process::Command;

use crate::ui;
use crate::version;

const MIN_VERSION: &str = "0.39.0";
const INSTALL_URL: &str =
    "https://raw.githubusercontent.com/rtk-ai/rtk/refs/heads/master/install.sh";
const GITHUB_LATEST_URL: &str = "https://api.github.com/repos/rtk-ai/rtk/releases/latest";

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
}

pub fn latest_remote_version() -> Option<String> {
    let resp = ureq::get(GITHUB_LATEST_URL)
        .set("User-Agent", "whetstone")
        .call()
        .ok()?;
    let body = resp.into_string().ok()?;
    let release: GithubRelease = serde_json::from_str(&body).ok()?;
    let tag = release.tag_name.trim_start_matches('v');
    version::extract_semver(tag)
}

pub fn installed_version() -> Option<String> {
    let output = Command::new("rtk").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    version::extract_semver(&raw)
}

fn is_rtk_ai() -> bool {
    Command::new("rtk")
        .arg("gain")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn install(force: bool) -> Result<()> {
    if let Some(ver) = installed_version() {
        if is_rtk_ai() {
            if !force && !version::is_older(&ver, MIN_VERSION) {
                ui::ok(&format!("rtk {ver} (>= {MIN_VERSION})"));
                return Ok(());
            }
            ui::info(&format!("upgrading rtk from {ver}"));
        } else {
            ui::warn("found rtk binary but it's not rtk-ai (Rust Type Kit collision?)");
            ui::info("installing rtk-ai alongside existing binary");
        }
    } else {
        ui::info("installing rtk");
    }

    run_install()?;

    match installed_version() {
        Some(ver) if is_rtk_ai() => ui::ok(&format!("rtk {ver}")),
        _ => bail!("rtk installation completed but rtk-ai not detected"),
    }
    Ok(())
}

pub fn update() -> Result<ui::ComponentStatus> {
    let Some(old_ver) = installed_version() else {
        return Ok(ui::ComponentStatus::NotInstalled);
    };
    if !is_rtk_ai() {
        return Ok(ui::ComponentStatus::Failed(
            "not rtk-ai (collision?)".into(),
        ));
    }

    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("curl -fsSL {INSTALL_URL} | sh"))
        .output()
        .context("failed to run rtk install")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("rtk installation failed: {stderr}");
    }

    let new_ver = installed_version().unwrap_or_else(|| old_ver.clone());
    if new_ver != old_ver {
        Ok(ui::ComponentStatus::Updated(old_ver, new_ver))
    } else {
        Ok(ui::ComponentStatus::UpToDate(old_ver))
    }
}

fn run_install() -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("curl -fsSL {INSTALL_URL} | sh"))
        .status()?;

    if !status.success() {
        bail!("rtk installation failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_release() {
        let json = r#"{"tag_name":"v0.42.0"}"#;
        let release: GithubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v0.42.0");
        let tag = release.tag_name.trim_start_matches('v');
        assert_eq!(version::extract_semver(tag), Some("0.42.0".into()));
    }

    #[test]
    fn parse_github_release_no_v_prefix() {
        let json = r#"{"tag_name":"0.42.0"}"#;
        let release: GithubRelease = serde_json::from_str(json).unwrap();
        let tag = release.tag_name.trim_start_matches('v');
        assert_eq!(version::extract_semver(tag), Some("0.42.0".into()));
    }
}
