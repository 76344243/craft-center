//! What the Craft-Zentrale knows and does, without any UI: the apps, the state of their forks
//! on GitHub, what is installed on this Mac, and installing a fork release.
//!
//! It shares its bookkeeping with the ArtCraft-Updater (`craft-update.sh`): the same state
//! folder, the same `<repo>.installed` stamps and the same lock, so the two never install at
//! the same time and each knows what the other installed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

/// The GitHub account holding the forks.
pub const OWNER: &str = "76344243";
/// The fork branch the Mac builds come from.
pub const BRANCH: &str = "mein-stand";

/// One app the Craft-Zentrale looks after.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSpec {
    /// The repository name, the same in the fork and in the original (`wordcraft`).
    pub repo: &'static str,
    /// The app's name, as in `/Applications/<name>.app`.
    pub name: &'static str,
    /// The original repository the fork follows (`storytold/wordcraft`), if any.
    pub upstream: Option<&'static str>,
}

/// The apps, in display order. Keep in step with `APPS` in `craft-update.sh`.
pub const APPS: &[AppSpec] = &[
    AppSpec {
        repo: "wordcraft",
        name: "WordCraft",
        upstream: Some("storytold/wordcraft"),
    },
    AppSpec {
        repo: "pdfcraft",
        name: "PdfCraft",
        upstream: Some("storytold/pdfcraft"),
    },
    AppSpec {
        repo: "craft-center",
        name: "CraftCenter",
        upstream: None,
    },
];

// ---- The fork on GitHub ------------------------------------------------------------------------

/// The newest release of a fork.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    /// When it was published (RFC 3339, UTC).
    pub published: Option<String>,
    pub page: String,
    /// The `.dmg` to install from.
    pub dmg: Option<String>,
}

/// How the fork's branch compares with the original's `main`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drift {
    /// Changes in the original that the fork doesn't have yet (the next weekly run takes them).
    pub behind: u64,
    /// The fork's own changes on top.
    pub ahead: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkState {
    pub release: Result<Release, String>,
    /// `None` for an app without an original, or when GitHub didn't say.
    pub drift: Option<Result<Drift, String>>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn github_json(url: &str) -> Result<serde_json::Value, String> {
    let mut response = agent()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header(
            "User-Agent",
            concat!("CraftCenter/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("GitHub nicht erreichbar ({e})"))?;
    let status = response.status().as_u16();
    if status == 403 || status == 429 {
        return Err(
            "GitHub bremst gerade (stündliches Abfrage-Limit). Später noch einmal prüfen.".into(),
        );
    }
    if status == 404 {
        return Err("noch keine Version veröffentlicht".into());
    }
    if !(200..300).contains(&status) {
        return Err(format!("GitHub antwortet mit Fehler {status}"));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(4 << 20)
        .read_to_string()
        .map_err(|e| format!("unlesbare Antwort ({e})"))?;
    serde_json::from_str(&body).map_err(|e| format!("unlesbare Antwort ({e})"))
}

/// Reads a `releases/latest` answer.
pub fn parse_release(v: &serde_json::Value) -> Result<Release, String> {
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        tag_name: String,
        published_at: Option<String>,
        html_url: String,
        #[serde(default)]
        assets: Vec<Asset>,
    }
    let raw: Raw = serde_json::from_value(v.clone())
        .map_err(|_| "unerwartete Antwort von GitHub".to_string())?;
    let dmg = raw
        .assets
        .into_iter()
        .find(|a| a.name.ends_with(".dmg"))
        .map(|a| a.browser_download_url);
    Ok(Release {
        tag: raw.tag_name,
        published: raw.published_at,
        page: raw.html_url,
        dmg,
    })
}

/// Reads a `compare` answer.
pub fn parse_drift(v: &serde_json::Value) -> Result<Drift, String> {
    let n = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "unerwartete Antwort von GitHub".to_string())
    };
    Ok(Drift {
        behind: n("behind_by")?,
        ahead: n("ahead_by")?,
    })
}

/// Asks GitHub about one app's fork (blocking).
pub fn fork_state(app: &AppSpec) -> ForkState {
    let release = github_json(&format!(
        "https://api.github.com/repos/{OWNER}/{}/releases/latest",
        app.repo
    ))
    .and_then(|v| parse_release(&v));
    let drift = app.upstream.map(|up| {
        github_json(&format!(
            "https://api.github.com/repos/{up}/compare/main...{OWNER}:{BRANCH}"
        ))
        .and_then(|v| parse_drift(&v))
    });
    ForkState { release, drift }
}

// ---- What is installed ---------------------------------------------------------------------

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// The ArtCraft-Updater's folder: install stamps and the lock.
pub fn state_dir() -> PathBuf {
    home().join("Library/Application Support/ArtCraft-Updater")
}

/// Whether the ArtCraft-Updater is set up (its LaunchAgent exists).
pub fn auto_updates_on() -> bool {
    home()
        .join("Library/LaunchAgents/com.76344243.craft-updater.plist")
        .exists()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub path: PathBuf,
    /// `CFBundleShortVersionString`.
    pub version: Option<String>,
    /// The fork release installed, if the Craft-Zentrale or the Updater installed it.
    pub tag: Option<String>,
    /// When the app was put in place (local time, for display).
    pub when: Option<chrono::DateTime<chrono::Local>>,
}

/// Where an app is or would go: where it already is, else /Applications.
pub fn app_path(name: &str) -> PathBuf {
    let system = PathBuf::from(format!("/Applications/{name}.app"));
    let user = home().join(format!("Applications/{name}.app"));
    if !system.exists() && user.exists() {
        user
    } else {
        system
    }
}

fn stamp_path(repo: &str) -> PathBuf {
    state_dir().join(format!("{repo}.installed"))
}

/// The installed copy of `app`, if there is one.
pub fn installed(app: &AppSpec) -> Option<Installed> {
    let path = app_path(app.name);
    if !path.is_dir() {
        return None;
    }
    let version = plist::Value::from_file(path.join("Contents/Info.plist"))
        .ok()
        .and_then(|v| {
            v.as_dictionary()?
                .get("CFBundleShortVersionString")?
                .as_string()
                .map(str::to_string)
        });
    let tag = std::fs::read_to_string(stamp_path(app.repo))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let when = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .map(chrono::DateTime::<chrono::Local>::from);
    Some(Installed {
        path,
        version,
        tag,
        when,
    })
}

/// How the installed copy compares with the fork's newest release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Match {
    /// The newest release is installed.
    Same,
    /// A newer release is waiting.
    UpdateAvailable,
    /// Installed, but not by the Craft-Zentrale or the Updater: which release it is isn't known.
    Unknown,
    /// Not installed.
    Missing,
    /// The fork has no release to compare with.
    NoRelease,
}

pub fn compare(installed: Option<&Installed>, release: Option<&Release>) -> Match {
    match (installed, release) {
        (_, None) => Match::NoRelease,
        (None, Some(_)) => Match::Missing,
        (Some(i), Some(r)) => match &i.tag {
            Some(t) if *t == r.tag => Match::Same,
            Some(_) => Match::UpdateAvailable,
            None => Match::Unknown,
        },
    }
}

// ---- Installing ----------------------------------------------------------------------------

/// Where an install is.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    WaitingForQuit,
    Downloading { done: u64, total: Option<u64> },
    Installing,
    Done { tag: String },
    Failed(String),
}

impl Step {
    pub fn finished(&self) -> bool {
        matches!(self, Step::Done { .. } | Step::Failed(_))
    }
}

/// Whether the app is running (any of its windows: WordCraft opens one process per window).
pub fn running(name: &str) -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-f", &format!("{name}.app/Contents/MacOS/")])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Asks the app to quit, the way the Dock's Quit does (it asks about unsaved work itself).
pub fn ask_to_quit(name: &str) {
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", &format!("tell application \"{name}\" to quit")])
        .output();
}

/// The Updater's lock: held while either installs. Released when dropped.
struct Lock(PathBuf);

impl Lock {
    fn take() -> Result<Lock, String> {
        let dir = state_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Ordner {} nicht anlegbar ({e})", dir.display()))?;
        let lock = dir.join("lock");
        for _ in 0..120 {
            if std::fs::create_dir(&lock).is_ok() {
                let _ = std::fs::write(lock.join("pid"), format!("{}\n", std::process::id()));
                return Ok(Lock(lock));
            }
            if lock_is_stale(&lock) {
                let _ = std::fs::remove_dir_all(&lock);
                continue;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        Err(
            "Der automatische Updater installiert gerade. Bitte gleich noch einmal versuchen."
                .into(),
        )
    }
}

/// Whether the lock was left behind: the process named in it no longer runs, or (no name in
/// it) it is older than ten minutes. Same rule as `craft-update.sh`.
fn lock_is_stale(lock: &Path) -> bool {
    match std::fs::read_to_string(lock.join("pid"))
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok())
    {
        Some(pid) => !Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .output()
            .is_ok_and(|o| o.status.success()),
        None => std::fs::metadata(lock)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > Duration::from_secs(600)),
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!(
            "{what} fehlgeschlagen{}",
            if err.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", err.trim())
            }
        ))
    }
}

/// Installs `release` of `app`, reporting each step. Blocking: run it on its own thread.
/// The old app stays in place until the new one is completely copied.
pub fn install(
    app: &AppSpec,
    release: &Release,
    quit_first: bool,
    report: &dyn Fn(Step),
) -> Result<String, String> {
    let url = release
        .dmg
        .as_deref()
        .ok_or("Diese Version hat keine .dmg")?;
    if quit_first && running(app.name) {
        report(Step::WaitingForQuit);
        ask_to_quit(app.name);
        let mut waited = 0;
        while running(app.name) {
            if waited >= 120 {
                return Err(format!(
                    "{} wurde nicht beendet (vielleicht wartet ein Dokument aufs Speichern).",
                    app.name
                ));
            }
            std::thread::sleep(Duration::from_secs(1));
            waited += 1;
        }
    } else if app.name != SELF_NAME && running(app.name) {
        return Err(format!("{} läuft gerade.", app.name));
    }
    let _lock = Lock::take()?;
    let tmp =
        std::env::temp_dir().join(format!("craft-center-{}-{}", app.repo, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("Zwischenordner: {e}"))?;
    let result = install_in(app, release, url, &tmp, report);
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// This app's own name: it may replace itself while running (macOS allows it), then restarts.
pub const SELF_NAME: &str = "CraftCenter";

fn install_in(
    app: &AppSpec,
    release: &Release,
    url: &str,
    tmp: &Path,
    report: &dyn Fn(Step),
) -> Result<String, String> {
    // Download. curl and this app set no quarantine flag, so macOS asks nothing on first start.
    report(Step::Downloading {
        done: 0,
        total: None,
    });
    let mut response = agent()
        .get(url)
        .header(
            "User-Agent",
            concat!("CraftCenter/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| format!("Download nicht möglich ({e})"))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("Download fehlgeschlagen (Fehler {status})"));
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let dmg = tmp.join("app.dmg");
    {
        let mut file = std::fs::File::create(&dmg).map_err(|e| format!("Zwischendatei: {e}"))?;
        let mut reader = response.body_mut().with_config().limit(2 << 30).reader();
        let mut buf = vec![0u8; 1 << 16];
        let mut done = 0u64;
        let mut last = 0u64;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("Download abgebrochen ({e})"))?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n])
                .map_err(|e| format!("Zwischendatei: {e}"))?;
            done += n as u64;
            if done - last >= 256 * 1024 {
                last = done;
                report(Step::Downloading { done, total });
            }
        }
        report(Step::Downloading { done, total });
    }

    report(Step::Installing);
    let mount = tmp.join("mnt");
    std::fs::create_dir_all(&mount).map_err(|e| format!("Zwischenordner: {e}"))?;
    run(
        Command::new("/usr/bin/hdiutil")
            .args(["attach", "-nobrowse", "-readonly", "-quiet", "-mountpoint"])
            .arg(&mount)
            .arg(&dmg),
        "Öffnen der .dmg",
    )?;
    let source = mount.join(format!("{}.app", app.name));
    let target = app_path(app.name);
    let dest = target
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("/Applications"));
    let staged = dest.join(format!(".{}.app.new", app.name));
    let copied = if source.is_dir() {
        let _ = std::fs::remove_dir_all(&staged);
        std::fs::create_dir_all(&dest)
            .map_err(|e| format!("{}: {e}", dest.display()))
            .and_then(|()| {
                run(
                    Command::new("/usr/bin/ditto").arg(&source).arg(&staged),
                    "Kopieren",
                )
            })
    } else {
        Err(format!("{}.app fehlt in der .dmg", app.name))
    };
    let _ = run(
        Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet"])
            .arg(&mount),
        "Schließen der .dmg",
    );
    if let Err(e) = copied {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(e);
    }
    let _ = Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&staged)
        .output();
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staged);
            format!("Alte Version nicht ersetzbar ({e})")
        })?;
    }
    std::fs::rename(&staged, &target).map_err(|e| format!("Neue Version nicht ablegbar ({e})"))?;
    let _ = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister").arg("-f").arg(&target).output();
    std::fs::write(stamp_path(app.repo), format!("{}\n", release.tag))
        .map_err(|e| format!("Vermerk nicht speicherbar ({e})"))?;
    Ok(release.tag.clone())
}

/// Starts the app (after an update that quit it, or this app after updating itself).
pub fn launch(name: &str, new_instance: bool) {
    let mut cmd = Command::new("/usr/bin/open");
    if new_instance {
        cmd.arg("-n");
    }
    let _ = cmd.arg(app_path(name)).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release(tag: &str) -> Release {
        Release {
            tag: tag.into(),
            published: None,
            page: String::new(),
            dmg: Some("https://x/y.dmg".into()),
        }
    }

    fn inst(tag: Option<&str>) -> Installed {
        Installed {
            path: PathBuf::from("/Applications/X.app"),
            version: Some("1.0".into()),
            tag: tag.map(Into::into),
            when: None,
        }
    }

    #[test]
    fn a_release_answer_gives_its_tag_and_first_dmg() {
        let v = json!({
            "tag_name": "mein-stand-20261024-3", "published_at": "2026-10-24T05:15:00Z", "html_url": "https://github.com/a/b/releases/tag/t",
            "body": null,
            "assets": [{"name": "notes.txt", "browser_download_url": "https://n", "label": null}, {"name": "pdfcraft-0.13.0-macos-aarch64.dmg", "browser_download_url": "https://d"}]
        });
        let r = parse_release(&v).unwrap();
        assert_eq!(r.tag, "mein-stand-20261024-3");
        assert_eq!(r.dmg.as_deref(), Some("https://d"));
        assert!(parse_release(&json!({"message": "Not Found"})).is_err());
    }

    #[test]
    fn a_compare_answer_gives_behind_and_ahead() {
        assert_eq!(
            parse_drift(&json!({"ahead_by": 3, "behind_by": 12})).unwrap(),
            Drift {
                behind: 12,
                ahead: 3
            }
        );
        assert!(parse_drift(&json!({})).is_err());
    }

    #[test]
    fn installed_and_released_compare() {
        let r = release("b");
        assert_eq!(compare(Some(&inst(Some("b"))), Some(&r)), Match::Same);
        assert_eq!(
            compare(Some(&inst(Some("a"))), Some(&r)),
            Match::UpdateAvailable
        );
        assert_eq!(compare(Some(&inst(None)), Some(&r)), Match::Unknown);
        assert_eq!(compare(None, Some(&r)), Match::Missing);
        assert_eq!(compare(Some(&inst(Some("b"))), None), Match::NoRelease);
    }

    #[test]
    fn a_lock_whose_process_is_gone_is_stale() {
        let dir = std::env::temp_dir().join(format!("cc-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!lock_is_stale(&dir), "a fresh lock without a pid is kept");
        std::fs::write(dir.join("pid"), format!("{}\n", std::process::id())).unwrap();
        assert!(!lock_is_stale(&dir), "our own process is running");
        std::fs::write(dir.join("pid"), "999999999\n").unwrap();
        assert!(lock_is_stale(&dir), "a process that doesn't exist");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_app_list_matches_the_updater() {
        let script = include_str!("../updater/craft-update.sh");
        for app in APPS {
            assert!(
                script.contains(&format!("\"{}:{}\"", app.repo, app.name)),
                "{} fehlt in craft-update.sh",
                app.repo
            );
        }
    }
}
