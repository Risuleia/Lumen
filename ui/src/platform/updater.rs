use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, anyhow};
use lumen_core::cache_dir;
use self_update::cargo_crate_version;

use crate::platform::toast::show_update_toast;

const GITHUB_OWNER: &str = "Risuleia";
const GITHUB_REPO: &str = "Lumen";
const CHECK_INTERVAL_SECS: u64 = 60 * 60 * 24;

#[derive(Clone, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    NotAvailable,
    Available(String),
    Downloading,
    Failed,
}

pub fn start_update_check() {
    std::thread::spawn(|| {
        if let Some(version) = check_for_update() {
            show_update_toast(&version, || {
                if let Err(e) = download_and_apply_update() {
                    eprintln!("[Updater] Update download failed: {e}");
                }
            });
        }
    });
}

pub fn check_for_update() -> Option<String> {
    if !should_check() {
        return None;
    }

    record_check();

    let current = cargo_crate_version!();

    let release = self_update::backends::github::Update::configure()
        .repo_owner(GITHUB_OWNER)
        .repo_name(GITHUB_REPO)
        .bin_name("Lumen")
        .current_version(current)
        .build()
        .ok()?
        .get_latest_release()
        .ok()?;

    if self_update::version::bump_is_greater(current, &release.version).unwrap_or(false) {
        Some(release.version)
    } else {
        None
    }
}

pub fn force_check_for_update() -> Option<String> {
    let current = cargo_crate_version!();

    let release = self_update::backends::github::Update::configure()
        .repo_owner(GITHUB_OWNER)
        .repo_name(GITHUB_REPO)
        .bin_name("Lumen")
        .current_version(current)
        .build()
        .ok()?
        .get_latest_release()
        .ok()?;

    record_check();

    if self_update::version::bump_is_greater(current, &release.version).unwrap_or(false) {
        Some(release.version)
    } else {
        None
    }
}

pub fn download_and_apply_update() -> Result<()> {
    let current = cargo_crate_version!();

    let release = self_update::backends::github::Update::configure()
        .repo_owner(GITHUB_OWNER)
        .repo_name(GITHUB_REPO)
        .bin_name("Lumen")
        .current_version(current)
        .build()?
        .get_latest_release()?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with("-setup.exe"))
        .ok_or_else(|| anyhow!("No installer asset was found in release"))?;

    let tmp_dir = std::env::temp_dir();
    let installer_path = tmp_dir.join(&asset.name);

    {
        let mut installer_file = fs::File::create(&installer_path)?;
        self_update::Download::from_url(&asset.download_url)
            .set_header(
                "Accept".parse()?, 
                "application/octet-stream".parse()?
            )
            .download_to(&mut installer_file)?;
        installer_file.sync_all()?;
    }

    const DETACHED_PROCESS: u32 = 0x00000008;

    std::process::Command::new(&installer_path)
        .arg("/SILENT")
        .arg("/CLOSEAPPLICATIONS")
        .creation_flags(DETACHED_PROCESS)
        .spawn()?;

    Ok(())
}

fn should_check() -> bool {
    let path = last_check_path();

    let Ok(contents) = std::fs::read_to_string(&path) else {
        return true;
    };
    let Ok(ts) = contents.trim().parse::<u64>() else {
        return true;
    };

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    now.saturating_sub(ts) >= CHECK_INTERVAL_SECS
}

fn record_check() {
    std::fs::create_dir_all(cache_dir()).ok();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let _ = std::fs::write(last_check_path(), now.to_string());
}

fn last_check_path() -> PathBuf {
    cache_dir().join("last_update_check")
}
