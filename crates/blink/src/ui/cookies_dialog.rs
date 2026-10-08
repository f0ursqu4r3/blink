//! The cookie jar. Port of `CookiesDialog.vue`.

use std::collections::HashSet;

use blink_core::engine::StoredCookie;
use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::widgets::{WIDEST, tracked};

/// Open the dialog.
pub fn open(store: Entity<Store>, window: &mut Window, cx: &mut App) {
    let jar = cx.new(|cx| CookieJar::new(store, window, cx));
    let filter = jar.read(cx).filter.clone();
    window.open_dialog(cx, move |dialog, _, cx| {
        let colors = theme::colors(cx);
        let clear = jar.clone();
        let empty = jar.read(cx).cookies.is_empty();
        dialog
            .w(px(760.))
            // `top-1/2 -translate-y-1/2 max-h-[80dvh]`.
            .centered(true)
            .max_h(relative(0.8))
            .p_0()
            .close_button(false)
            .title(jar.read(cx).render_header(jar.clone(), cx).w_full())
            .child(div().mt(px(-8.)).child(jar.clone()))
            .footer(
                h_flex()
                    .mt(px(-8.))
                    .justify_between()
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(colors.border)
                    .px(px(16.))
                    .py(px(12.))
                    .child(
                        secondary_button("clear-cookies", "Clear all")
                            .disabled(empty)
                            .on_click(move |_, _, cx| {
                                clear.update(cx, |jar, cx| jar.clear(cx));
                            }),
                    )
                    .child(
                        secondary_button("close-cookies", "Close")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
    });
    window.defer(cx, move |window, cx| {
        filter.update(cx, |input, cx| input.focus(window, cx));
    });
}

/// A Vue `variant="secondary"` button: `h-7 px-2.5 font-mono text-xs
/// font-medium`, bordered.
fn secondary_button(id: &'static str, label: &'static str) -> Button {
    Button::new(id)
        .outline()
        .xsmall()
        .h(px(28.))
        .px(px(10.))
        .font_family(theme::MONO)
        .font_weight(FontWeight::MEDIUM)
        .label(label)
}

/// Cookies whose domain, name, or value contains `query`, without case.
fn filter_cookies<'a>(cookies: &'a [StoredCookie], query: &str) -> Vec<&'a StoredCookie> {
    let needle = query.trim().to_lowercase();
    cookies
        .iter()
        .filter(|cookie| {
            needle.is_empty()
                || [&cookie.domain, &cookie.name, &cookie.value]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&needle))
        })
        .collect()
}

fn domain_count(cookies: &[StoredCookie]) -> usize {
    cookies
        .iter()
        .map(|cookie| cookie.domain.as_str())
        .collect::<HashSet<_>>()
        .len()
}

/// The local expiry, such as "9/30/26, 3:45 PM"; "Session" for a session
/// cookie.
fn expiry(cookie: &StoredCookie) -> String {
    match cookie.expires {
        Some(ms) => blink_core::history::short_date_time(ms),
        None => "Session".into(),
    }
}

fn flags(cookie: &StoredCookie) -> String {
    [
        cookie.secure.then_some("Secure"),
        cookie.http_only.then_some("HttpOnly"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

struct CookieJar {
    store: Entity<Store>,
    cookies: Vec<StoredCookie>,
    filter: Entity<InputState>,
    error: String,
    project: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl CookieJar {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx
            .new(|cx| InputState::new(window, cx).placeholder("Filter by domain, name, or value"));
        let subscriptions = vec![cx.subscribe(&filter, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        })];
        let project = store.read(cx).active_project_path();
        let cookies = store
            .read(cx)
            .engine
            .list_cookies_scoped(project.as_deref())
            .unwrap_or_default();
        CookieJar {
            store,
            cookies,
            filter,
            error: String::new(),
            project,
            _subscriptions: subscriptions,
        }
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        self.cookies = self
            .store
            .read(cx)
            .engine
            .list_cookies_scoped(self.project.as_deref())
            .unwrap_or_default();
        cx.notify();
    }

    fn run(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.error.clear(),
            Err(error) => self.error = error,
        }
        self.load(cx);
    }

    fn delete(&mut self, cookie: &StoredCookie, cx: &mut Context<Self>) {
        let result = self.store.read(cx).engine.delete_cookie_scoped(
            self.project.as_deref(),
            &cookie.domain,
            &cookie.path,
            &cookie.name,
        );
        self.run(result, cx);
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let result = self
            .store
            .read(cx)
            .engine
            .clear_cookies_scoped(self.project.as_deref());
        self.run(result, cx);
    }

    /// Title, count, storage notice, and filter: these stay put while the
    /// table scrolls.
    fn render_header(&self, _jar: Entity<Self>, cx: &App) -> Div {
        let colors = theme::colors(cx);
        let enabled = self
            .store
            .read(cx)
            .workspace
            .preferences
            .transport
            .store_cookies;
        v_flex()
            .child(
                h_flex()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border)
                    .px(px(16.))
                    .py(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(colors.foreground)
                            .child(tracked(if self.project.is_some() { "Project Cookies" } else { "Local Cookies" }, 0.08)),
                    )
                    .child(
                        div()
                            .font_family(theme::MONO)
                            .text_size(px(10.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(colors.muted_foreground)
                            .child(tracked(
                                format!(
                                    "{} COOKIES · {} DOMAINS",
                                    self.cookies.len(),
                                    domain_count(&self.cookies)
                                ),
                                0.1,
                            )),
                    ),
            )
            .when(!enabled, |this| {
                this.child(
                    div()
                        .border_b_1()
                        .border_color(colors.border)
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(11.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(colors.warning)
                        .child(
                            "Cookie storage is off in Application Settings. Blink does not send or keep these cookies.",
                        ),
                )
            })
            .child(
                div()
                    .border_b_1()
                    .border_color(colors.border)
                    .px(px(16.))
                    .py(px(8.))
                    .font_weight(FontWeight::NORMAL)
                    .font_family(theme::MONO)
                    .child(
                        Input::new(&self.filter)
                            .small()
                            .h(px(30.))
                            .prefix(
                                Icon::new(IconName::Search)
                                    .size(px(13.))
                                    .text_color(colors.muted_foreground),
                            ),
                    ),
            )
    }
}

impl Render for CookieJar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let query = self.filter.read(cx).value().to_string();
        let shown: Vec<StoredCookie> = filter_cookies(&self.cookies, &query)
            .into_iter()
            .cloned()
            .collect();
        let head = |text: &'static str| {
            div()
                .px(px(12.))
                .py(px(8.))
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.muted_foreground)
                .child(tracked(text.to_uppercase(), WIDEST))
        };
        let cell = || div().px(px(12.)).py(px(6.)).min_w_0();
        let rows = shown.into_iter().map(|cookie| {
            let key = format!("{} {} {}", cookie.domain, cookie.path, cookie.name);
            let flags = flags(&cookie);
            let label = format!("Delete cookie {} for {}", cookie.name, cookie.domain);
            let target = cookie.clone();
            h_flex()
                .id(SharedString::from(format!("cookie-{key}")))
                .items_start()
                .border_b_1()
                .border_color(colors.border)
                .child(cell().w(relative(0.24)).flex_none().child(
                    div().flex().flex_wrap().child(cookie.domain.clone()).when(
                        cookie.path != "/",
                        |this| {
                            this.child(
                                div()
                                    .text_color(colors.muted_foreground)
                                    .child(cookie.path.clone()),
                            )
                        },
                    ),
                ))
                .child(
                    cell()
                        .w(relative(0.18))
                        .flex_none()
                        .text_color(colors.info)
                        .child(cookie.name.clone()),
                )
                .child(
                    cell()
                        .flex_1()
                        .child(
                            div()
                                .id(SharedString::from(format!("cookie-value-{key}")))
                                .line_clamp(2)
                                .child(cookie.value.clone())
                                .tooltip({
                                    let value = cookie.value.clone();
                                    move |window, cx| {
                                        gpui_kit::component::tooltip::Tooltip::new(value.clone())
                                            .build(window, cx)
                                    }
                                }),
                        )
                        .when(!flags.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(px(9.))
                                    .text_color(colors.muted_foreground)
                                    .child(flags),
                            )
                        }),
                )
                .child(
                    cell()
                        .w(relative(0.18))
                        .flex_none()
                        .text_color(colors.muted_foreground)
                        .child(expiry(&cookie)),
                )
                .child(
                    div().w(px(36.)).flex_none().p(px(2.)).child(
                        Button::new(SharedString::from(format!("delete-{key}")))
                            .ghost()
                            .small()
                            .size(px(28.))
                            .text_color(colors.muted_foreground)
                            .icon(Icon::new(IconName::X).size(px(13.)))
                            .accessibility_label(label)
                            .on_click(cx.listener(move |this, _, _, cx| this.delete(&target, cx))),
                    ),
                )
        });
        let empty_text = if self.cookies.is_empty() {
            "No cookies. Responses that set cookies add them here."
        } else {
            "No cookies match this filter."
        };
        let has_rows = !filter_cookies(&self.cookies, &query).is_empty();
        v_flex()
            .font_family(theme::MONO)
            .text_size(px(11.))
            .text_color(colors.foreground)
            .child(
                h_flex()
                    .bg(colors.muted)
                    .child(head("Domain").w(relative(0.24)).flex_none())
                    .child(head("Name").w(relative(0.18)).flex_none())
                    .child(head("Value").flex_1())
                    .child(head("Expires").w(relative(0.18)).flex_none())
                    .child(div().w(px(36.)).flex_none()),
            )
            .children(rows)
            .when(!has_rows, |this| {
                this.child(
                    div()
                        .p(px(16.))
                        .font_family(theme::SANS)
                        .text_size(px(12.))
                        .text_color(colors.muted_foreground)
                        .child(empty_text),
                )
            })
            .when(!self.error.is_empty(), |this| {
                this.child(
                    div()
                        .px(px(16.))
                        .py(px(8.))
                        .font_family(theme::SANS)
                        .text_size(px(12.))
                        .text_color(colors.destructive)
                        .child(self.error.clone()),
                )
            })
    }
}

#[cfg(test)]
mod ui_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn cookie(domain: &str, name: &str, value: &str) -> StoredCookie {
        StoredCookie {
            domain: domain.into(),
            path: "/".into(),
            name: name.into(),
            value: value.into(),
            expires: None,
            secure: false,
            http_only: false,
        }
    }

    #[test]
    fn filters_by_domain_name_or_value_without_case() {
        let cookies = vec![
            cookie("api.example.test", "session", "abc"),
            cookie("other.test", "theme", "DARK"),
        ];
        assert_eq!(filter_cookies(&cookies, " ").len(), 2);
        assert_eq!(filter_cookies(&cookies, "EXAMPLE").len(), 1);
        assert_eq!(filter_cookies(&cookies, "dark")[0].name, "theme");
        assert!(filter_cookies(&cookies, "missing").is_empty());
    }

    #[test]
    fn counts_distinct_domains() {
        let cookies = vec![
            cookie("a.test", "one", ""),
            cookie("a.test", "two", ""),
            cookie("b.test", "three", ""),
        ];
        assert_eq!(domain_count(&cookies), 2);
    }

    #[test]
    fn writes_session_and_dated_expiry() {
        let mut item = cookie("a.test", "one", "");
        assert_eq!(expiry(&item), "Session");
        item.expires = Some(1_790_783_100_000);
        assert_eq!(
            expiry(&item),
            blink_core::history::short_date_time(1_790_783_100_000)
        );
    }

    #[test]
    fn joins_the_secure_and_http_only_flags() {
        let mut item = cookie("a.test", "one", "");
        assert_eq!(flags(&item), "");
        item.secure = true;
        item.http_only = true;
        assert_eq!(flags(&item), "Secure · HttpOnly");
    }
}
