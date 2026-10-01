//! The update flow with a stand-in for GitHub Releases.

use std::collections::VecDeque;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use core::prelude::v1::test;
use gpui_kit::{AppContext as _, Entity, TestAppContext};

use super::{
    FIRST_CHECK, NOT_INSTALLED, QuitToInstall, Release, Releases, Retry, Status, UP_TO_DATE_FOR,
    Updater,
};
use crate::test_support;
use crate::ui::update_notice::notice_text;

#[derive(Default)]
struct Fake {
    checks: Mutex<VecDeque<Result<Option<Release>, String>>>,
    downloads: Mutex<VecDeque<Result<(), String>>>,
    installs: Mutex<Vec<String>>,
}

impl Fake {
    fn new(
        checks: impl IntoIterator<Item = Result<Option<Release>, String>>,
        downloads: impl IntoIterator<Item = Result<(), String>>,
    ) -> Arc<Self> {
        Arc::new(Fake {
            checks: Mutex::new(checks.into_iter().collect()),
            downloads: Mutex::new(downloads.into_iter().collect()),
            installs: Mutex::default(),
        })
    }
}

impl Releases for Fake {
    fn check(&self) -> Result<Option<Release>, String> {
        self.checks.lock().unwrap().pop_front().unwrap_or(Ok(None))
    }

    fn download(&self, _: &Release, progress: Sender<i16>) -> Result<(), String> {
        for percent in [40, 100] {
            progress.send(percent).unwrap();
        }
        self.downloads.lock().unwrap().pop_front().unwrap_or(Ok(()))
    }

    fn install_after_quit(&self, release: &Release) -> Result<(), String> {
        self.installs.lock().unwrap().push(release.version.clone());
        Ok(())
    }
}

fn release(version: &str) -> Release {
    Release {
        version: version.into(),
    }
}

fn updater(
    fake: Option<Arc<Fake>>,
    dir: &tempfile::TempDir,
    cx: &mut TestAppContext,
) -> Entity<Updater> {
    let engine = test_support::engine(dir.path());
    cx.new(|cx| Updater::new(fake.map(|fake| fake as Arc<dyn Releases>), engine, cx))
}

fn wait(cx: &mut TestAppContext, time: Duration) {
    cx.executor().advance_clock(time);
    cx.run_until_parked();
}

fn notice(updater: &Entity<Updater>, cx: &TestAppContext) -> Option<Status> {
    cx.read(|cx| updater.read(cx).notice().cloned())
}

#[gpui_kit::test]
fn an_automatic_check_offers_a_release_until_it_is_skipped(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new([Ok(Some(release("0.2.0")))], []);
    let first = updater(Some(fake), &dir, cx);
    cx.run_until_parked();
    assert_eq!(notice(&first, cx), None, "the first check waits");
    wait(cx, FIRST_CHECK);
    assert_eq!(
        notice(&first, cx),
        Some(Status::Available(release("0.2.0")))
    );

    first.update(cx, |updater, cx| updater.skip(cx));
    assert_eq!(notice(&first, cx), None);

    // After a restart the automatic check finds it again and stays quiet,
    // but a manual check shows it.
    let fake = Fake::new([Ok(Some(release("0.2.0"))), Ok(Some(release("0.2.0")))], []);
    let next = updater(Some(fake), &dir, cx);
    wait(cx, FIRST_CHECK);
    assert_eq!(
        cx.read(|cx| next.read(cx).status().clone()),
        Status::Available(release("0.2.0"))
    );
    assert_eq!(notice(&next, cx), None);
    next.update(cx, |updater, cx| updater.check(true, cx));
    cx.run_until_parked();
    assert_eq!(notice(&next, cx), Some(Status::Available(release("0.2.0"))));
}

#[gpui_kit::test]
fn downloads_then_restart_installs_after_quitting(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake::new(
        [Ok(Some(release("0.2.0")))],
        [
            Err("Cannot download the update. Check the network connection.".into()),
            Ok(()),
        ],
    );
    let updater = updater(Some(fake.clone()), &dir, cx);
    let quits = Arc::new(Mutex::new(0));
    let counted = quits.clone();
    cx.update(|cx| {
        cx.subscribe(&updater, move |_, _: &QuitToInstall, _| {
            *counted.lock().unwrap() += 1
        })
        .detach()
    });
    updater.update(cx, |updater, cx| updater.check(true, cx));
    cx.run_until_parked();

    // A failed download offers to try again.
    updater.update(cx, |updater, cx| updater.download(cx));
    assert_eq!(
        notice(&updater, cx),
        Some(Status::Downloading {
            release: release("0.2.0"),
            percent: 0
        })
    );
    wait(cx, Duration::from_secs(1));
    assert_eq!(
        notice(&updater, cx),
        Some(Status::Failed {
            message: "Cannot download the update. Check the network connection.".into(),
            retry: Some(Retry::Download(release("0.2.0"))),
        })
    );
    updater.update(cx, |updater, cx| updater.retry(cx));
    wait(cx, Duration::from_secs(1));
    assert_eq!(notice(&updater, cx), Some(Status::Ready(release("0.2.0"))));

    // Later hides it; a manual check brings it back without a new check.
    updater.update(cx, |updater, cx| updater.dismiss(cx));
    assert_eq!(notice(&updater, cx), None);
    updater.update(cx, |updater, cx| updater.check(true, cx));
    assert_eq!(notice(&updater, cx), Some(Status::Ready(release("0.2.0"))));

    updater.update(cx, |updater, cx| updater.restart(cx));
    assert_eq!(*fake.installs.lock().unwrap(), ["0.2.0"]);
    assert_eq!(*quits.lock().unwrap(), 1);
    assert_eq!(
        notice(&updater, cx),
        Some(Status::Installing(release("0.2.0")))
    );
}

#[gpui_kit::test]
fn failed_automatic_checks_stay_quiet_and_manual_ones_say_so(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let error = || Err("Cannot check for updates. Check the network connection.".to_string());
    let fake = Fake::new([error(), error(), Ok(None)], []);
    let updater = updater(Some(fake), &dir, cx);
    wait(cx, FIRST_CHECK);
    assert_eq!(notice(&updater, cx), None);

    updater.update(cx, |updater, cx| updater.check(true, cx));
    assert_eq!(notice(&updater, cx), Some(Status::Checking));
    cx.run_until_parked();
    assert_eq!(
        notice(&updater, cx),
        Some(Status::Failed {
            message: "Cannot check for updates. Check the network connection.".into(),
            retry: Some(Retry::Check),
        })
    );
    updater.update(cx, |updater, cx| updater.retry(cx));
    cx.run_until_parked();
    assert_eq!(notice(&updater, cx), Some(Status::UpToDate));
    wait(cx, UP_TO_DATE_FOR);
    assert_eq!(notice(&updater, cx), None);
}

#[gpui_kit::test]
fn a_build_not_installed_from_a_release_cannot_update(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let updater = updater(None, &dir, cx);
    wait(cx, FIRST_CHECK);
    updater.update(cx, |updater, cx| updater.check(false, cx));
    assert_eq!(notice(&updater, cx), None);
    updater.update(cx, |updater, cx| updater.check(true, cx));
    assert_eq!(
        notice(&updater, cx),
        Some(Status::Failed {
            message: NOT_INSTALLED.into(),
            retry: None,
        })
    );
}

#[test]
fn notices_name_the_versions() {
    let current = env!("CARGO_PKG_VERSION");
    assert_eq!(
        notice_text(&Status::Available(release("9.0.0"))),
        format!("Blink 9.0.0 is available. You have {current}.")
    );
    assert_eq!(
        notice_text(&Status::Downloading {
            release: release("9.0.0"),
            percent: 40
        }),
        "Downloading Blink 9.0.0… 40%"
    );
    assert_eq!(
        release("9.0.0").page(),
        "https://github.com/f0ursqu4r3/blink/releases/tag/v9.0.0"
    );
}
