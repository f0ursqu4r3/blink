//! Updates from GitHub Releases: check, download, then quit and open the
//! new version.
//!
//! Velopack does the work. It needs a build installed from a Velopack
//! package (see `.github/workflows/release.yml`), so `cargo run` and
//! `script/bundle-macos` builds have no updates.

use std::sync::mpsc::{Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use blink_core::engine::{Engine, UpdateChoices};
use gpui_kit::*;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

pub const REPOSITORY: &str = "https://github.com/f0ursqu4r3/blink";
/// The first automatic check waits for the app to settle.
const FIRST_CHECK: Duration = Duration::from_secs(5);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// "Up to date" after a manual check goes away by itself.
const UP_TO_DATE_FOR: Duration = Duration::from_secs(4);
const PROGRESS_POLL: Duration = Duration::from_millis(100);
pub const NOT_INSTALLED: &str =
    "Updates work in Blink installed from a GitHub release. This build cannot update itself.";

/// A newer release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
}

impl Release {
    /// The release page, with its notes.
    pub fn page(&self) -> String {
        format!("{REPOSITORY}/releases/tag/v{}", self.version)
    }
}

/// The release service, so tests can stand in for GitHub. Each call blocks.
pub trait Releases: Send + Sync + 'static {
    /// The newest release when it is newer than this build.
    fn check(&self) -> Result<Option<Release>, String>;
    /// Download `release`, sending the percent done.
    fn download(&self, release: &Release, progress: Sender<i16>) -> Result<(), String>;
    /// Start the helper that installs `release` after Blink quits, then
    /// opens Blink again.
    fn install_after_quit(&self, release: &Release) -> Result<(), String>;
}

/// Velopack with GitHub Releases as the source.
struct Velopack {
    manager: UpdateManager,
    /// The last update found, which download and install need.
    found: Mutex<Option<UpdateInfo>>,
}

impl Velopack {
    /// None when this build is not installed from a Velopack package.
    fn open() -> Option<Self> {
        let source = GithubSource::new(REPOSITORY, None, false);
        let manager = UpdateManager::new(source, None, None).ok()?;
        Some(Velopack {
            manager,
            found: Mutex::new(None),
        })
    }

    fn found(&self, release: &Release) -> Result<UpdateInfo, String> {
        self.found
            .lock()
            .map_err(|_| "The update state is not available.".to_string())?
            .clone()
            .filter(|info| info.TargetFullRelease.Version == release.version)
            .ok_or_else(|| "Check for updates again.".to_string())
    }
}

impl Releases for Velopack {
    fn check(&self) -> Result<Option<Release>, String> {
        let check = self
            .manager
            .check_for_updates()
            .map_err(|error| failure("Cannot check for updates", &error))?;
        let UpdateCheck::UpdateAvailable(info) = check else {
            return Ok(None);
        };
        let release = Release {
            version: info.TargetFullRelease.Version.clone(),
        };
        if let Ok(mut found) = self.found.lock() {
            *found = Some(*info);
        }
        Ok(Some(release))
    }

    fn download(&self, release: &Release, progress: Sender<i16>) -> Result<(), String> {
        let info = self.found(release)?;
        self.manager
            .download_updates(&info, Some(progress))
            .map_err(|error| failure("Cannot download the update", &error))
    }

    fn install_after_quit(&self, release: &Release) -> Result<(), String> {
        let info = self.found(release)?;
        self.manager
            .wait_exit_then_apply_updates(&info, false, true, Vec::<String>::new())
            .map_err(|error| failure("Cannot install the update", &error))
    }
}

fn failure(action: &str, error: &velopack::Error) -> String {
    match error {
        velopack::Error::Network(_) => format!("{action}. Check the network connection."),
        error => format!("{action}: {error}"),
    }
}

/// What to do again after a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retry {
    Check,
    Download(Release),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Idle,
    Checking,
    /// After a manual check found nothing newer.
    UpToDate,
    Available(Release),
    Downloading {
        release: Release,
        percent: u8,
    },
    Ready(Release),
    /// The helper waits for Blink to quit.
    Installing(Release),
    Failed {
        message: String,
        retry: Option<Retry>,
    },
}

/// Ask the app to quit, so the helper can install the update.
pub struct QuitToInstall;

impl EventEmitter<QuitToInstall> for Updater {}

pub struct Updater {
    releases: Option<Arc<dyn Releases>>,
    engine: Engine,
    choices: UpdateChoices,
    status: Status,
    /// The user asked for the current check, so it shows every result.
    manual: bool,
    /// "Later": hide the notice until the next check finds a release.
    hidden: bool,
    task: Option<Task<()>>,
    _schedule: Option<Task<()>>,
}

impl Updater {
    /// Velopack when this build is installed from a release.
    pub fn installed(engine: Engine, cx: &mut Context<Self>) -> Self {
        let releases = Velopack::open().map(|velopack| Arc::new(velopack) as Arc<dyn Releases>);
        Self::new(releases, engine, cx)
    }

    /// `releases` None: this build cannot update itself.
    pub fn new(
        releases: Option<Arc<dyn Releases>>,
        engine: Engine,
        cx: &mut Context<Self>,
    ) -> Self {
        let schedule = releases.is_some().then(|| {
            cx.spawn(async move |this, cx| {
                let mut wait = FIRST_CHECK;
                loop {
                    cx.background_executor().timer(wait).await;
                    wait = CHECK_EVERY;
                    if this.update(cx, |this, cx| this.check(false, cx)).is_err() {
                        break;
                    }
                }
            })
        });
        Updater {
            choices: engine.load_update_choices(),
            releases,
            engine,
            status: Status::Idle,
            manual: false,
            hidden: false,
            task: None,
            _schedule: schedule,
        }
    }

    #[cfg(test)]
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The status to show, or None to show nothing.
    pub fn notice(&self) -> Option<&Status> {
        let shown = match &self.status {
            Status::Idle => false,
            Status::Checking | Status::UpToDate | Status::Failed { .. } => self.manual,
            Status::Available(release) => {
                !self.hidden
                    && (self.manual
                        || self.choices.skipped_version.as_deref() != Some(&release.version))
            }
            Status::Ready(_) => !self.hidden,
            Status::Downloading { .. } | Status::Installing(_) => true,
        };
        shown.then_some(&self.status)
    }

    fn set(&mut self, status: Status, cx: &mut Context<Self>) {
        self.status = status;
        cx.notify();
    }

    /// Look for a newer release. A manual check shows every result; an
    /// automatic one shows only a release the user has not skipped.
    pub fn check(&mut self, manual: bool, cx: &mut Context<Self>) {
        match &self.status {
            // A check would replace work in progress; show it instead.
            Status::Downloading { .. } | Status::Ready(_) | Status::Installing(_) => {
                if manual {
                    self.hidden = false;
                    cx.notify();
                }
                return;
            }
            Status::Checking => {
                self.manual |= manual;
                cx.notify();
                return;
            }
            _ => {}
        }
        self.manual = manual;
        let Some(releases) = self.releases.clone() else {
            if manual {
                self.set(
                    Status::Failed {
                        message: NOT_INSTALLED.into(),
                        retry: None,
                    },
                    cx,
                );
            }
            return;
        };
        self.set(Status::Checking, cx);
        let job = cx.background_spawn(async move { releases.check() });
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| this.checked(result, cx)).ok();
        }));
    }

    fn checked(&mut self, result: Result<Option<Release>, String>, cx: &mut Context<Self>) {
        match result {
            Ok(Some(release)) => {
                self.hidden = false;
                self.set(Status::Available(release), cx);
            }
            Ok(None) if self.manual => {
                self.set(Status::UpToDate, cx);
                self.task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(UP_TO_DATE_FOR).await;
                    this.update(cx, |this, cx| {
                        if this.status == Status::UpToDate {
                            this.set(Status::Idle, cx);
                        }
                    })
                    .ok();
                }));
            }
            Ok(None) => self.set(Status::Idle, cx),
            Err(message) => self.set(
                Status::Failed {
                    message,
                    retry: Some(Retry::Check),
                },
                cx,
            ),
        }
    }

    /// Download the available release.
    pub fn download(&mut self, cx: &mut Context<Self>) {
        let release = match &self.status {
            Status::Available(release) => release.clone(),
            Status::Failed {
                retry: Some(Retry::Download(release)),
                ..
            } => release.clone(),
            _ => return,
        };
        let Some(releases) = self.releases.clone() else {
            return;
        };
        // The user started it, so a failure shows.
        self.manual = true;
        self.set(
            Status::Downloading {
                release: release.clone(),
                percent: 0,
            },
            cx,
        );
        let (sender, receiver) = channel();
        let job_release = release.clone();
        let job = cx.background_spawn(async move { releases.download(&job_release, sender) });
        self.task = Some(cx.spawn(async move |this, cx| {
            // The download drops its sender when it ends.
            loop {
                cx.background_executor().timer(PROGRESS_POLL).await;
                let mut latest = None;
                let ended = loop {
                    match receiver.try_recv() {
                        Ok(percent) => latest = Some(percent),
                        Err(TryRecvError::Empty) => break false,
                        Err(TryRecvError::Disconnected) => break true,
                    }
                };
                if let Some(percent) = latest {
                    this.update(cx, |this, cx| this.progress(percent, cx)).ok();
                }
                if ended {
                    break;
                }
            }
            let result = job.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.set(Status::Ready(release), cx),
                Err(message) => this.set(
                    Status::Failed {
                        message,
                        retry: Some(Retry::Download(release)),
                    },
                    cx,
                ),
            })
            .ok();
        }));
    }

    fn progress(&mut self, percent: i16, cx: &mut Context<Self>) {
        if let Status::Downloading { percent: shown, .. } = &mut self.status {
            *shown = percent.clamp(0, 100) as u8;
            cx.notify();
        }
    }

    /// Start the installer, then ask the app to quit. The installer opens
    /// the new version after Blink quits.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        let Status::Ready(release) = &self.status else {
            return;
        };
        let Some(releases) = self.releases.clone() else {
            return;
        };
        let release = release.clone();
        match releases.install_after_quit(&release) {
            Ok(()) => {
                self.set(Status::Installing(release), cx);
                cx.emit(QuitToInstall);
            }
            Err(message) => self.set(
                Status::Failed {
                    message,
                    retry: None,
                },
                cx,
            ),
        }
    }

    pub fn retry(&mut self, cx: &mut Context<Self>) {
        match &self.status {
            Status::Failed {
                retry: Some(Retry::Check),
                ..
            } => self.check(true, cx),
            Status::Failed {
                retry: Some(Retry::Download(_)),
                ..
            } => self.download(cx),
            _ => {}
        }
    }

    /// Close the notice. An available or downloaded release comes back
    /// with the next check.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        match self.status {
            Status::Available(_) | Status::Ready(_) => self.hidden = true,
            Status::UpToDate | Status::Failed { .. } => self.status = Status::Idle,
            _ => {}
        }
        cx.notify();
    }

    /// Do not offer the available release again in automatic checks.
    pub fn skip(&mut self, cx: &mut Context<Self>) {
        let Status::Available(release) = &self.status else {
            return;
        };
        self.choices.skipped_version = Some(release.version.clone());
        // Not saved: the release shows again after the next start.
        let _ = self.engine.save_update_choices(&self.choices);
        self.manual = false;
        self.set(Status::Idle, cx);
    }
}

#[cfg(test)]
mod tests;
