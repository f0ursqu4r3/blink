//! Application settings. Port of `ApplicationSettingsDialog.vue` and `ThemeSettings.vue`.

pub(crate) mod form;
mod theme_settings;

use blink_core::definitions::{definitions_to_rows, rows_to_definitions};
use blink_core::model::{BodyMode, METHODS, Pair, TransportOptions, WorkspacePreferences};
use blink_core::preferences::default_preferences;
use blink_core::transport_options::{
    TransportField, TransportFieldErrors, proxy_url_error, transport_field_errors,
};
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::select::Select;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use self::form::{
    Choice, ChoiceSelect, check, u, choice_select, dialog_footer, dialog_header, field_label,
    footer_button, note, section_heading, section_heading_with_help, selected,
};
use self::theme_settings::ThemeSettings;
use crate::store::Store;
use crate::theme::{self, AppTheme};
use crate::ui::key_value_editor::{KeyValueEditor, KeyValueEvent, KeyValueOptions};

/// Open the dialog.
pub fn open(store: Entity<Store>, window: &mut Window, cx: &mut App) {
    // A fresh dialog starts from the saved theme.
    theme::update_theme(cx, |theme| theme.state.revert());
    let settings = cx.new(|cx| Settings::new(store, window, cx));
    let method = settings.read(cx).method.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
        let confirm = settings.clone();
        let save = settings.clone();
        let colors = theme::colors(cx);
        let viewport = window.viewport_size().height;
        dialog
            // `w-[min(560px,…)]` at the current zoom; centered with at most
            // `max-h-[90dvh]`, so the top margin is 5% of the viewport.
            .w(u(560.).to_pixels(window.rem_size()))
            .margin_top(viewport * 0.05)
            .max_h(viewport * 0.9)
            .p_0()
            .close_button(false)
            .overlay_closable(false)
            .title(dialog_header("Application Settings", cx).w_full())
            .child(div().mt(u(-8.)).child(settings.clone()))
            .footer(
                dialog_footer(cx)
                    .mt(u(-8.))
                    .child(
                        footer_button("settings-cancel", "Cancel", false).on_click(
                            |_, window, cx| {
                                theme::update_theme(cx, |theme| theme.state.revert());
                                window.close_dialog(cx);
                            },
                        ),
                    )
                    .child(footer_button("settings-save", "Save", true).on_click(
                        move |_, window, cx| {
                            if save.update(cx, |settings, cx| settings.save(window, cx)) {
                                window.close_dialog(cx);
                            }
                        },
                    ))
                    .text_color(colors.foreground),
            )
            .on_ok(move |_, window, cx| confirm.update(cx, |settings, cx| settings.save(window, cx)))
            .on_cancel(move |_, _, cx| {
                theme::update_theme(cx, |theme| theme.state.revert());
                true
            })
    });
    // As `open-auto-focus`: the default method takes focus.
    window.defer(cx, move |window, cx| {
        method.update(cx, |select, cx| select.focus(window, cx));
    });
}

const NUMBER_FIELDS: [(TransportField, &str, &str, Option<&str>); 4] = [
    (TransportField::TimeoutSeconds, "app-timeout", "Timeout (s)", None),
    (
        TransportField::ConnectTimeoutSeconds,
        "app-connect-timeout",
        "Connect timeout (s)",
        None,
    ),
    (TransportField::MaxRedirects, "app-max-redirects", "Max redirects", None),
    (
        TransportField::InspectionLimitMiB,
        "app-inspection-limit",
        "Inspection limit (MiB)",
        Some("Larger bodies show a truncated preview. Save the response to get the full body."),
    ),
];

fn body_mode_choices() -> Vec<Choice> {
    vec![
        Choice::new("none", "None"),
        Choice::new("json", "JSON"),
        Choice::new("text", "Text"),
        Choice::new("graphql", "GraphQL"),
    ]
}

/// A typed number, or `u64::MAX` when it is not a whole number, so the
/// range check reports it the way the Vue check reports NaN or a fraction.
fn parse_number(text: &str) -> u64 {
    text.trim().parse::<u64>().unwrap_or(u64::MAX)
}

fn set_field(options: &mut TransportOptions, field: TransportField, value: u64) {
    match field {
        TransportField::TimeoutSeconds => options.timeout_seconds = value,
        TransportField::ConnectTimeoutSeconds => options.connect_timeout_seconds = value,
        TransportField::MaxRedirects => options.max_redirects = value,
        TransportField::InspectionLimitMiB => options.inspection_limit_mi_b = value,
    }
}

/// Field errors that block a save. Max redirects is disabled (and reset on
/// save) while redirects are off, so its error does not count then.
fn relevant_errors(options: &TransportOptions) -> TransportFieldErrors {
    let mut errors = transport_field_errors(options);
    if !options.follow_redirects {
        errors.remove(&TransportField::MaxRedirects);
    }
    errors
}

/// The preferences to save, or None while a field is invalid.
fn validated(mut next: WorkspacePreferences) -> Option<WorkspacePreferences> {
    if !relevant_errors(&next.transport).is_empty()
        || !proxy_url_error(&next.transport.proxy_url).is_empty()
    {
        return None;
    }
    next.transport.proxy_url = next.transport.proxy_url.trim().to_string();
    if !next.transport.follow_redirects
        && transport_field_errors(&next.transport).contains_key(&TransportField::MaxRedirects)
    {
        next.transport.max_redirects = default_preferences().transport.max_redirects;
    }
    Some(next)
}

struct Settings {
    store: Entity<Store>,
    /// The saved preferences the draft started from.
    base: WorkspacePreferences,
    method: Entity<ChoiceSelect>,
    body_mode: Entity<ChoiceSelect>,
    pretty: bool,
    wrap: bool,
    follow_redirects: bool,
    verify_tls: bool,
    store_cookies: bool,
    confirm_close_drafts: bool,
    numbers: Vec<(TransportField, Entity<InputState>)>,
    proxy_url: Entity<InputState>,
    tokens: Entity<KeyValueEditor>,
    token_rows: Vec<Pair>,
    theme: Entity<ThemeSettings>,
    /// Errors show only after a save attempt, so typing does not flash errors.
    submitted: bool,
    token_error: String,
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = &store.read(cx).workspace;
        let base = workspace.preferences.clone();
        let token_rows = definitions_to_rows(&workspace.global_definitions);
        let methods = METHODS.iter().map(|m| Choice::same(*m)).collect();
        let method = choice_select(methods, &base.default_method, window, cx);
        let body_mode = choice_select(body_mode_choices(), base.default_body_mode.id(), window, cx);
        let numbers = NUMBER_FIELDS
            .iter()
            .map(|(field, ..)| {
                let value = field.get(&base.transport).to_string();
                let input = cx.new(|cx| InputState::new(window, cx).default_value(value));
                (*field, input)
            })
            .collect();
        let proxy = base.transport.proxy_url.clone();
        let proxy_url = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("System proxy")
                .default_value(proxy)
        });
        let tokens = cx.new(|cx| {
            KeyValueEditor::new(
                token_rows.clone(),
                KeyValueOptions {
                    key_label: "Name".into(),
                    value_label: "Value".into(),
                    key_placeholder: "Name".into(),
                    value_placeholder: "Value".into(),
                    allow_files: false,
                    toggles: false,
                    id: "global-tokens".into(),
                },
                window,
                cx,
            )
        });
        let subscriptions = vec![cx.subscribe(&tokens, |this, _, event: &KeyValueEvent, cx| {
            if let KeyValueEvent::Change(rows) = event {
                this.token_rows = rows.clone();
                cx.notify();
            }
        })];
        let theme = cx.new(|cx| ThemeSettings::new(store.clone(), window, cx));
        Settings {
            store,
            pretty: base.pretty,
            wrap: base.wrap,
            follow_redirects: base.transport.follow_redirects,
            verify_tls: base.transport.verify_tls,
            store_cookies: base.transport.store_cookies,
            confirm_close_drafts: base.confirm_close_drafts,
            base,
            method,
            body_mode,
            numbers,
            proxy_url,
            tokens,
            token_rows,
            theme,
            submitted: false,
            token_error: String::new(),
            _subscriptions: subscriptions,
        }
    }

    /// The preferences as edited, before validation.
    fn draft(&self, cx: &App) -> WorkspacePreferences {
        let mut next = self.base.clone();
        next.default_method = selected(&self.method, cx);
        if next.default_method.is_empty() {
            next.default_method = self.base.default_method.clone();
        }
        next.default_body_mode =
            BodyMode::from_id(&selected(&self.body_mode, cx)).unwrap_or(self.base.default_body_mode);
        next.pretty = self.pretty;
        next.wrap = self.wrap;
        next.confirm_close_drafts = self.confirm_close_drafts;
        let transport = &mut next.transport;
        transport.follow_redirects = self.follow_redirects;
        transport.verify_tls = self.verify_tls;
        transport.store_cookies = self.store_cookies;
        transport.proxy_url = self.proxy_url.read(cx).value().to_string();
        for (field, input) in &self.numbers {
            set_field(transport, *field, parse_number(&input.read(cx).value()));
        }
        next
    }

    /// Save, as the Vue `save()`. True when the dialog may close.
    fn save(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.submitted = true;
        cx.notify();
        let rows = if self.token_rows.is_empty() {
            self.tokens.read(cx).rows().to_vec()
        } else {
            self.token_rows.clone()
        };
        let definitions = match rows_to_definitions(&rows) {
            Ok(definitions) => {
                self.token_error.clear();
                definitions
            }
            Err(error) => {
                self.token_error = error;
                return false;
            }
        };
        if !cx.global::<AppTheme>().state.error.is_empty() {
            return false;
        }
        let Some(next) = validated(self.draft(cx)) else {
            return false;
        };
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                workspace.set_global_definitions(definitions);
                workspace.set_preferences(next);
            })
        });
        let failed = theme::update_theme(cx, |theme| {
            let mut text = None;
            let error = theme.state.commit(|value| {
                text = Some(value);
                Ok(())
            });
            if let Some(value) = text
                && let Err(_) = theme.write(value)
            {
                theme.state.save_error = blink_core::theme::STORAGE_FAILURE.into();
                return true;
            }
            !error.is_empty()
        });
        !failed
    }

    fn manage_cookies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        theme::update_theme(cx, |theme| theme.state.revert());
        window.close_dialog(cx);
        window.defer(cx, |window, cx| {
            window.dispatch_action(Box::new(crate::actions::ManageCookies), cx);
        });
    }

    fn render_number(
        &self,
        index: usize,
        errors: &TransportFieldErrors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (field, _, label, help) = NUMBER_FIELDS[index];
        let colors = theme::colors(cx);
        let input = &self.numbers[index].1;
        let disabled = field == TransportField::MaxRedirects && !self.follow_redirects;
        let error = errors.get(&field).cloned();
        v_flex()
            .gap(u(6.))
            .child(field_label(label, cx))
            .child(
                Input::new(input)
                    .font_family(theme::MONO)
                    .text_size(u(12.))
                    .disabled(disabled)
                    .when(error.is_some(), |this| this.border_color(colors.destructive)),
            )
            .map(|this| match (error, help) {
                (Some(error), _) => this.child(note(error, colors.destructive).text_size(u(12.))),
                (None, Some(help)) => this.child(note(help, colors.muted_foreground)),
                _ => this,
            })
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let draft = self.draft(cx);
        let (errors, proxy_error) = if self.submitted {
            (
                relevant_errors(&draft.transport),
                proxy_url_error(&draft.transport.proxy_url),
            )
        } else {
            (TransportFieldErrors::new(), String::new())
        };
        let muted = colors.muted_foreground;
        let two_columns = || div().grid().grid_cols(2).gap(u(12.)).text_size(u(12.));

        let defaults = two_columns()
            .child(div().col_span_full().child(section_heading_with_help(
                "defaults-help",
                "New request defaults",
                "These settings apply only when you create a new request. They do not change existing requests or duplicates.",
                cx,
            )))
            .child(
                v_flex()
                    .gap(u(6.))
                    .child(field_label("Method", cx))
                    .child(Select::new(&self.method).font_family(theme::MONO).text_size(u(12.))),
            )
            .child(
                v_flex()
                    .gap(u(6.))
                    .child(field_label("Body mode", cx))
                    .child(Select::new(&self.body_mode).font_family(theme::MONO).text_size(u(12.))),
            )
            .child(check("app-pretty", "Format responses", self.pretty).on_click(cx.listener(
                |this, checked: &bool, _, cx| {
                    this.pretty = *checked;
                    cx.notify();
                },
            )))
            .child(check("app-wrap", "Wrap response lines", self.wrap).on_click(cx.listener(
                |this, checked: &bool, _, cx| {
                    this.wrap = *checked;
                    cx.notify();
                },
            )));

        let numbers: Vec<_> = (0..NUMBER_FIELDS.len())
            .map(|index| self.render_number(index, &errors, cx).into_any_element())
            .collect();
        let requests = two_columns()
            .border_t_1()
            .border_color(colors.border)
            .pt(u(12.))
            .child(div().col_span_full().child(section_heading_with_help(
                "requests-help",
                "Requests",
                "These limits apply to every request you send. The download limit is 1 GiB.",
                cx,
            )))
            .child(
                div().col_span_full().child(
                    check("app-follow-redirects", "Follow redirects", self.follow_redirects)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.follow_redirects = *checked;
                            cx.notify();
                        })),
                ),
            )
            .children(numbers)
            .child(
                v_flex()
                    .col_span_full()
                    .gap(u(6.))
                    .child(field_label("Proxy URL", cx))
                    .child(
                        Input::new(&self.proxy_url)
                            .font_family(theme::MONO)
                            .text_size(u(12.))
                            .when(!proxy_error.is_empty(), |this| {
                                this.border_color(colors.destructive)
                            }),
                    )
                    .map(|this| {
                        if proxy_error.is_empty() {
                            this.child(note(
                                "http, https, or socks5. Empty uses the system proxy settings. Desktop only.",
                                muted,
                            ))
                        } else {
                            this.child(note(proxy_error, colors.destructive).text_size(u(12.)))
                        }
                    }),
            )
            .child(
                v_flex()
                    .col_span_full()
                    .gap(u(4.))
                    .child(
                        check("app-verify-tls", "Verify TLS certificates", self.verify_tls)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.verify_tls = *checked;
                                cx.notify();
                            })),
                    )
                    .child(if self.verify_tls {
                        note(
                            "Blink trusts the system certificate store, including company and local development CAs.",
                            muted,
                        )
                    } else {
                        note(
                            "Blink accepts invalid and self-signed certificates for every request. Turn this on again when you finish.",
                            colors.warning,
                        )
                    }),
            )
            .child(
                v_flex()
                    .col_span_full()
                    .gap(u(4.))
                    .child(
                        h_flex()
                            .justify_between()
                            .gap(u(8.))
                            .child(
                                check("app-store-cookies", "Store and send cookies", self.store_cookies)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        this.store_cookies = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("manage-cookies")
                                    .text_size(u(11.))
                                    .text_color(muted)
                                    .underline()
                                    .cursor_pointer()
                                    .hover(move |style| style.text_color(colors.foreground))
                                    .child("Manage cookies…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.manage_cookies(window, cx)
                                    })),
                            ),
                    )
                    .child(note(
                        "One jar for all requests, kept on this device. Desktop only.",
                        muted,
                    )),
            );

        let workspace = v_flex()
            .gap(u(8.))
            .border_t_1()
            .border_color(colors.border)
            .pt(u(12.))
            .text_size(u(12.))
            .child(section_heading("Workspace", cx))
            .child(
                check(
                    "app-confirm-close",
                    "Confirm before deleting requests with content",
                    self.confirm_close_drafts,
                )
                .on_click(cx.listener(|this, checked: &bool, _, cx| {
                    this.confirm_close_drafts = *checked;
                    cx.notify();
                })),
            );

        let tokens = v_flex()
            .gap(u(8.))
            .border_t_1()
            .border_color(colors.border)
            .pt(u(12.))
            .child(section_heading_with_help(
                "token-help-global",
                "Global tokens",
                "Workspace-global tokens use {{_.name}}. Use {{!NAME}} in a value to read NAME from Blink's process environment when sending.",
                cx,
            ))
            .child(
                div()
                    .overflow_hidden()
                    .border_1()
                    .border_color(colors.input)
                    .rounded(u(2.))
                    .bg(colors.background)
                    .child(self.tokens.clone()),
            );

        v_flex()
            .p(u(16.))
            .gap(u(16.))
            .text_color(colors.foreground)
            .child(defaults)
            .child(requests)
            .child(workspace)
            .child(self.theme.clone())
            .child(tokens)
            .when(!self.token_error.is_empty(), |this| {
                this.child(
                    div()
                        .font_family(theme::MONO)
                        .text_size(u(12.))
                        .text_color(colors.destructive)
                        .child(self.token_error.clone()),
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

    fn with(change: impl FnOnce(&mut TransportOptions)) -> WorkspacePreferences {
        let mut next = default_preferences();
        change(&mut next.transport);
        next
    }

    #[test]
    fn reads_invalid_numbers_as_out_of_range() {
        assert_eq!(parse_number(" 30 "), 30);
        assert_eq!(parse_number("1.5"), u64::MAX);
        assert_eq!(parse_number(""), u64::MAX);
        let next = with(|t| t.timeout_seconds = parse_number("abc"));
        assert_eq!(
            transport_field_errors(&next.transport)[&TransportField::TimeoutSeconds],
            "Enter a whole number from 1 to 600."
        );
    }

    #[test]
    fn ignores_max_redirects_while_redirects_are_off() {
        let next = with(|t| t.max_redirects = 0);
        assert!(relevant_errors(&next.transport).is_empty());
        let saved = validated(next).unwrap();
        assert_eq!(saved.transport.max_redirects, 10);
    }

    #[test]
    fn blocks_an_invalid_max_redirects_while_redirects_are_on() {
        let next = with(|t| {
            t.follow_redirects = true;
            t.max_redirects = 0;
        });
        assert!(validated(next).is_none());
    }

    #[test]
    fn trims_the_proxy_url_and_rejects_bad_ones() {
        let saved = validated(with(|t| t.proxy_url = " http://127.0.0.1:8080 ".into())).unwrap();
        assert_eq!(saved.transport.proxy_url, "http://127.0.0.1:8080");
        assert!(validated(with(|t| t.proxy_url = "ftp://x".into())).is_none());
    }

    #[test]
    fn keeps_the_connect_timeout_within_the_timeout() {
        let next = with(|t| {
            t.timeout_seconds = 5;
            t.connect_timeout_seconds = 10;
        });
        assert!(validated(next).is_none());
    }
}
