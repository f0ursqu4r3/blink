//! The update notice below the title bar: a newer release, its download,
//! and the restart that installs it.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::status_bar::{css, notice_button};
use crate::updater::{Status, Updater};

/// The notice text for `status`.
pub fn notice_text(status: &Status) -> String {
    let current = env!("CARGO_PKG_VERSION");
    match status {
        Status::Idle => String::new(),
        Status::Checking => "Checking for updates…".into(),
        Status::UpToDate => format!("Blink {current} is up to date."),
        Status::Available(release) => {
            format!(
                "Blink {} is available. You have {current}.",
                release.version
            )
        }
        Status::Downloading { release, percent } => {
            format!("Downloading Blink {}… {percent}%", release.version)
        }
        Status::Ready(release) => {
            format!("Blink {} is ready. Restart to install it.", release.version)
        }
        Status::Installing(release) => format!("Installing Blink {}…", release.version),
        Status::Failed { message, .. } => message.clone(),
    }
}

pub fn render(updater: &Entity<Updater>, cx: &mut App) -> AnyElement {
    let colors = theme::colors(cx);
    let Some(status) = updater.read(cx).notice().cloned() else {
        return div().into_any_element();
    };
    let on = |change: fn(&mut Updater, &mut Context<Updater>)| {
        let updater = updater.clone();
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| updater.update(cx, change)
    };
    let failed = matches!(status, Status::Failed { .. });
    let release = match &status {
        Status::Available(release) | Status::Ready(release) => Some(release.clone()),
        _ => None,
    };
    div()
        .id("update-notice")
        .flex()
        .flex_wrap()
        .items_center()
        .gap(css(8.))
        .px(css(16.))
        .py(css(8.))
        .border_b_1()
        .border_color(colors.border)
        .text_color(if failed {
            colors.destructive
        } else {
            colors.foreground
        })
        .text_size(css(12.))
        .child(div().flex_1().min_w(css(180.)).child(notice_text(&status)))
        .when_some(release, |this, release| {
            this.child(
                notice_button("update-notes", "What's new", false)
                    .on_click(move |_, _, cx| cx.open_url(&release.page())),
            )
        })
        .map(|this| match &status {
            Status::Available(_) => {
                this.child(
                    notice_button("update-download", "Update", true)
                        .on_click(on(Updater::download)),
                )
                .child(
                    notice_button("update-skip", "Skip this version", false)
                        .on_click(on(Updater::skip)),
                )
                .child(notice_button("update-later", "Later", false).on_click(on(Updater::dismiss)))
            }
            Status::Ready(_) => {
                this.child(
                    notice_button("update-restart", "Restart to update", true)
                        .on_click(on(Updater::restart)),
                )
                .child(notice_button("update-later", "Later", false).on_click(on(Updater::dismiss)))
            }
            Status::Failed { retry, .. } => this
                .when(retry.is_some(), |this| {
                    this.child(
                        notice_button("update-retry", "Retry", true).on_click(on(Updater::retry)),
                    )
                })
                .child(
                    notice_button("update-dismiss", "Dismiss", false)
                        .on_click(on(Updater::dismiss)),
                ),
            Status::UpToDate => this.child(
                notice_button("update-dismiss", "Dismiss", false).on_click(on(Updater::dismiss)),
            ),
            _ => this,
        })
        .into_any_element()
}
