//! Update check. The app asks GitHub for the latest published release when it
//! starts and twice a day after that; a newer one is announced with a desktop
//! notification and an Update button in the window. On macOS the button
//! installs the release in place and relaunches; elsewhere it opens the
//! download in the browser.

use crate::config;
use crate::core::{lock, Shared};
use eframe::egui;
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/PrettyPomodoro/PrettyPomodoro/releases/latest";
const RELEASES: &str = "https://github.com/PrettyPomodoro/PrettyPomodoro/releases/";
const CHECK_EVERY: Duration = Duration::from_secs(12 * 60 * 60);

/// Whether the Update button can install the release itself on this platform.
pub const INSTALLS_IN_PLACE: bool = cfg!(target_os = "macos");

/// The package for this platform, as named by the release workflow.
const ASSET: &str = if cfg!(target_os = "macos") {
    "PrettyPomodoro-macos-universal.zip"
} else if cfg!(target_os = "windows") {
    "PrettyPomodoro-windows-x64.zip"
} else {
    "PrettyPomodoro-linux-x64.tar.gz"
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Without the tag's leading "v".
    pub version: String,
    /// This platform's package, if the release has one.
    pub asset_url: Option<String>,
    pub page_url: String,
}

impl Release {
    /// Where the Update button sends the browser when the app cannot install
    /// the release itself.
    pub fn download_url(&self) -> &str {
        self.asset_url.as_deref().unwrap_or(&self.page_url)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    UpToDate,
    Available(Release),
    Installing(Release),
    Failed(Release, String),
}

impl UpdateState {
    pub fn release(&self) -> Option<&Release> {
        match self {
            UpdateState::UpToDate => None,
            UpdateState::Available(r) | UpdateState::Installing(r) | UpdateState::Failed(r, _) => Some(r),
        }
    }
}

/// "1.2.3" or "v1.2.3" as numbers; `None` for anything else, pre-releases included.
fn parse_version(text: &str) -> Option<Vec<u32>> {
    let parts: Option<Vec<u32>> = text.trim().trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

fn fetch_latest() -> Result<Release, String> {
    let json: serde_json::Value = ureq::get(LATEST)
        .timeout(Duration::from_secs(20))
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "pomodoro-rs")
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let version = json["tag_name"].as_str().ok_or("no tag name")?.trim_start_matches('v').to_owned();
    // Only links into this project's own releases are followed.
    let ours = |url: &&str| url.starts_with(RELEASES);
    let asset_url = json["assets"].as_array().into_iter().flatten().find_map(|asset| {
        let named = asset["name"].as_str() == Some(ASSET);
        asset["browser_download_url"].as_str().filter(|_| named).filter(ours).map(str::to_owned)
    });
    let page_url = json["html_url"].as_str().filter(ours).unwrap_or(RELEASES).to_owned();
    Ok(Release { version, asset_url, page_url })
}

/// Check now and then for as long as the app runs.
pub fn spawn_checker(shared: Shared, ctx: egui::Context) {
    std::thread::spawn(move || loop {
        if let Ok(release) = fetch_latest() {
            if is_newer(&release.version, env!("CARGO_PKG_VERSION")) {
                let mut core = lock(&shared);
                // Leave an install that is under way alone.
                if !matches!(core.update, UpdateState::Installing(_)) && core.update.release() != Some(&release) {
                    // The desktop notification comes once per version.
                    if core.cfg.update_notified != release.version {
                        core.notify(
                            "PrettyPomodoro update available",
                            &format!("Version {} is ready. Open the app and press Update.", release.version),
                        );
                        core.cfg.update_notified = release.version.clone();
                        config::save_config(&core.cfg);
                    }
                    core.update = UpdateState::Available(release);
                }
                drop(core);
                ctx.request_repaint();
            }
        }
        std::thread::sleep(CHECK_EVERY);
    });
}

/// Install the available release in the background, then relaunch.
pub fn spawn_install(shared: Shared, ctx: egui::Context) {
    let release = {
        let mut core = lock(&shared);
        let Some(release) = core.update.release().cloned() else {
            return;
        };
        if matches!(core.update, UpdateState::Installing(_)) {
            return;
        }
        core.update = UpdateState::Installing(release.clone());
        release
    };
    ctx.request_repaint();
    std::thread::spawn(move || {
        let error = match install(&release) {
            // Not reached on success: the new copy is launched and this one exits.
            Ok(()) => return,
            Err(e) => e,
        };
        lock(&shared).update = UpdateState::Failed(release, error);
        ctx.request_repaint();
    });
}

#[cfg(not(target_os = "macos"))]
fn install(_release: &Release) -> Result<(), String> {
    Err("This platform is updated from the download page.".into())
}

#[cfg(target_os = "macos")]
fn install(release: &Release) -> Result<(), String> {
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| e.to_string())?;
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .ok_or("The app is not running from an application bundle.")?
        .to_owned();
    macos::replace_bundle(&bundle, release)?;
    // Open the new copy once this one has gone.
    std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 1; open \"$0\""])
        .arg(&bundle)
        .spawn()
        .map_err(|e| e.to_string())?;
    std::process::exit(0);
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Release;
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;

    const EXECUTABLE: &str = "Contents/MacOS/prettypomodoro";

    fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
        let status = Command::new(program).args(args).status().map_err(|e| e.to_string())?;
        status.success().then_some(()).ok_or_else(|| format!("{program} failed."))
    }

    /// Download the release's package and put its app where `bundle` is. The
    /// new copy is staged beside the old one and swapped in by renaming, so a
    /// failure part-way leaves the old app in place.
    pub fn replace_bundle(bundle: &Path, release: &Release) -> Result<(), String> {
        let url = release.asset_url.as_deref().ok_or("This release has no macOS package.")?;
        let work = std::env::temp_dir().join(format!("prettypomodoro-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let result = (|| {
            let zip = work.join("update.zip");
            let response = ureq::get(url)
                .timeout(Duration::from_secs(300))
                .set("User-Agent", "pomodoro-rs")
                .call()
                .map_err(|e| format!("Download failed: {e}"))?;
            let mut file = std::fs::File::create(&zip).map_err(|e| e.to_string())?;
            std::io::copy(&mut response.into_reader(), &mut file).map_err(|e| format!("Download failed: {e}"))?;
            drop(file);

            let unpacked = work.join("unpacked");
            run("/usr/bin/ditto", &["-x".as_ref(), "-k".as_ref(), zip.as_os_str(), unpacked.as_os_str()])?;
            let name = bundle.file_name().ok_or("The app has no name.")?;
            let fresh = unpacked.join("PrettyPomodoro.app");
            if !fresh.join(EXECUTABLE).is_file() {
                return Err("The download does not contain the app.".to_owned());
            }

            let staged = bundle.with_file_name(format!("{}.new", name.to_string_lossy()));
            let retired = bundle.with_file_name(format!("{}.old", name.to_string_lossy()));
            let _ = std::fs::remove_dir_all(&staged);
            let _ = std::fs::remove_dir_all(&retired);
            run("/usr/bin/ditto", &[fresh.as_os_str(), staged.as_os_str()])
                .map_err(|_| "Could not write next to the app. Move it to Applications and try again.".to_owned())?;
            std::fs::rename(bundle, &retired).map_err(|e| e.to_string())?;
            if let Err(e) = std::fs::rename(&staged, bundle) {
                let _ = std::fs::rename(&retired, bundle);
                return Err(e.to_string());
            }
            let _ = std::fs::remove_dir_all(&retired);
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&work);
        result
    }

    /// Downloads the latest release from GitHub and installs it over a
    /// stand-in bundle; run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn installs_the_latest_release_over_a_bundle() {
        let release = super::fetch_latest().unwrap();
        let dir = std::env::temp_dir().join(format!("prettypomodoro-update-test-{}", std::process::id()));
        let bundle = dir.join("PrettyPomodoro.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(bundle.join(EXECUTABLE), "old").unwrap();

        replace_bundle(&bundle, &release).unwrap();

        let installed = std::fs::metadata(bundle.join(EXECUTABLE)).unwrap();
        assert!(installed.len() > 1_000_000, "the real executable is in place");
        let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains(&release.version), "the bundle is the released version");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(leftovers.len(), 1, "{leftovers:?}");
        let run = Command::new("/usr/bin/codesign").arg("--verify").arg(&bundle).status().unwrap();
        assert!(run.success(), "the installed bundle's signature is intact");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("0.2.0-beta", "0.1.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }
}
