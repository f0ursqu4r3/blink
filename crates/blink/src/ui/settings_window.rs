//! Application settings. Valid edits apply immediately and persist through Store.

mod layout;
mod placement;
mod theme_settings;

use blink_core::definitions::{definitions_to_rows, rows_to_definitions};
use blink_core::model::{BodyMode, METHODS, Pair, TransportOptions, WorkspacePreferences};
use blink_core::preferences::default_preferences;
use blink_core::transport_options::{
    TransportField, TransportFieldErrors, proxy_url_error, transport_field_errors,
};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use self::theme_settings::ThemeSettings;
use crate::store::Store;
use crate::theme;
use crate::ui::form::{
    Choice, ChoiceSelect, choice_select, footer_button, note, section_heading,
    section_heading_with_help, selected, u,
};
use crate::ui::key_value_editor::{KeyValueEditor, KeyValueEvent, KeyValueOptions};
use crate::ui::response_tokens_editor::{
    ResponseTokensChanged, ResponseTokensEditor, scoped_request_choices, self_supplied_warnings,
};
use crate::ui::widgets::tracked;

actions!(settings, [CancelSettings]);

const CONTEXT: &str = "SettingsWindow";
const TITLE: &str = "Blink Application Settings";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelSettings, Some(CONTEXT)),
        KeyBinding::new("secondary-w", CancelSettings, Some(CONTEXT)),
    ]);
}

/// The main window, so the settings window can hand work back to it.
pub struct MainWindow(pub AnyWindowHandle);

impl Global for MainWindow {}

/// The open settings window, if any. There is at most one.
struct OpenWindow(Option<WindowHandle<gpui_kit::base::Root>>);

impl Global for OpenWindow {}

fn open_window(cx: &App) -> Option<WindowHandle<gpui_kit::base::Root>> {
    cx.try_global::<OpenWindow>().and_then(|open| open.0)
}

/// Open the settings window, or bring it to the front when it is open.
pub fn open(store: Entity<Store>, window: &mut Window, cx: &mut App) {
    if let Some(handle) = open_window(cx)
        && handle
            .update(cx, |_, window, _| {
                placement::center_existing(window);
                window.activate_window();
            })
            .is_ok()
    {
        return;
    }
    // A fresh window starts from the saved theme.
    theme::update_theme(cx, |theme| theme.state.revert());
    let mut titlebar = TitleBar::title_bar_options();
    titlebar.title = Some(TITLE.into());
    titlebar.traffic_light_position = Some(crate::ui::title_bar::traffic_light_position());
    let (display_id, bounds) = placement::settings_bounds(window, cx);
    let options = WindowOptions {
        titlebar: Some(titlebar),
        display_id,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(gpui_kit::size(px(600.), px(420.))),
        is_minimizable: false,
        app_id: Some("com.kyle.blink.gpui".into()),
        ..TitleBar::window_options()
    };
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        let view = cx.new(|cx| SettingsWindow::new(store, window, cx));
        let weak = view.downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |view, cx| view.clear_preview(cx)).ok();
            // Closing clears only the temporary theme preview.
            theme::update_theme(cx, |theme| theme.state.revert());
            cx.set_global(OpenWindow(None));
            true
        });
        view
    });
    if let Ok((handle, _)) = opened {
        cx.set_global(OpenWindow(handle.downcast()));
    }
}

/// Close settings and restore the selected theme after a hover preview.
fn close(window: &mut Window, cx: &mut App) {
    theme::update_theme(cx, |theme| theme.state.revert());
    cx.set_global(OpenWindow(None));
    window.remove_window();
}

/// The window content: the title bar and settings panels.
pub struct SettingsWindow {
    settings: Entity<Settings>,
    focus_handle: FocusHandle,
}

impl SettingsWindow {
    fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = cx.new(|cx| Settings::new(store, window, cx));
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        SettingsWindow {
            settings,
            focus_handle,
        }
    }

    fn clear_preview(&self, cx: &mut App) {
        let theme = self.settings.read(cx).theme.clone();
        theme.update(cx, |theme, cx| theme.cancel_preview(cx));
    }

    fn cancel(&mut self, _: &CancelSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_preview(cx);
        close(window, cx);
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let mac = cfg!(target_os = "macos") && !window.is_fullscreen();
        v_flex()
            .id("settings-window")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::cancel))
            .size_full()
            .bg(colors.frame)
            .text_color(colors.foreground)
            .font_family(theme::SANS)
            .text_size(u(theme::FONT_SIZE))
            .child(
                TitleBar::new()
                    .h(px(crate::ui::title_bar::HEIGHT))
                    .bg(colors.frame)
                    .border_color(colors.frame)
                    .pl_0()
                    .child(
                        // Equal insets keep the title centered on the window,
                        // clear of the traffic lights.
                        h_flex()
                            .size_full()
                            .justify_center()
                            .when(mac, |this| this.px(px(72.)))
                            .font_family(theme::MONO)
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(colors.muted_foreground)
                            .child(tracked(TITLE.to_uppercase(), 0.08)),
                    ),
            )
            .child(self.settings.clone())
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

const NUMBER_FIELDS: [(TransportField, &str, &str, Option<&str>); 4] = [
    (
        TransportField::TimeoutSeconds,
        "app-timeout",
        "Timeout (s)",
        None,
    ),
    (
        TransportField::ConnectTimeoutSeconds,
        "app-connect-timeout",
        "Connect timeout (s)",
        None,
    ),
    (
        TransportField::MaxRedirects,
        "app-max-redirects",
        "Max redirects",
        None,
    ),
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    General,
    Appearance,
    Network,
    Tokens,
}

impl Page {
    const ALL: [Page; 4] = [Page::General, Page::Appearance, Page::Network, Page::Tokens];

    fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Network => "Network",
            Page::Tokens => "Tokens",
        }
    }

    fn description(self) -> Option<&'static str> {
        match self {
            Page::Network => {
                Some("These limits apply to every request you send. The download limit is 1 GiB.")
            }
            _ => None,
        }
    }
}

pub struct Settings {
    page: Page,
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
    applied_token_rows: Vec<Pair>,
    pub(crate) response_tokens: Entity<ResponseTokensEditor>,
    theme: Entity<ThemeSettings>,
    token_error: String,
    _subscriptions: Vec<Subscription>,
}

impl Settings {
    pub(crate) fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = &store.read(cx).workspace;
        let base = workspace.preferences.clone();
        let saved_tokens = workspace.global_response_tokens.clone();
        let requests = scoped_request_choices(workspace, None);
        let token_rows = definitions_to_rows(&workspace.global_definitions);
        let methods = METHODS.iter().map(|m| Choice::same(*m)).collect();
        let method = choice_select(methods, &base.default_method, window, cx);
        let body_mode = choice_select(body_mode_choices(), base.default_body_mode.id(), window, cx);
        let numbers: Vec<(TransportField, Entity<InputState>)> = NUMBER_FIELDS
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
        let warnings = self_supplied_warnings(&store.read(cx).workspace, &saved_tokens);
        let response_tokens =
            cx.new(|cx| ResponseTokensEditor::new(saved_tokens, requests, warnings, window, cx));
        let mut subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe(&tokens, |this, _, event: &KeyValueEvent, cx| {
                if let KeyValueEvent::Change(rows) = event {
                    this.token_rows = rows.clone();
                    this.apply_tokens(cx);
                }
            }),
            cx.subscribe(
                &response_tokens,
                |this, _, _: &ResponseTokensChanged, cx| {
                    this.apply_tokens(cx);
                },
            ),
        ];
        for select in [&method, &body_mode] {
            subscriptions.push(cx.subscribe(
                select,
                |this, _, _: &SelectEvent<Vec<Choice>>, cx| {
                    this.apply_preferences(cx);
                },
            ));
        }
        for input in numbers.iter().map(|(_, input)| input).chain([&proxy_url]) {
            subscriptions.push(cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.apply_preferences(cx);
                }
            }));
        }
        let theme = cx.new(|cx| ThemeSettings::new(store.clone(), window, cx));
        Settings {
            page: Page::General,
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
            applied_token_rows: token_rows.clone(),
            token_rows,
            response_tokens,
            theme,
            token_error: String::new(),
            _subscriptions: subscriptions,
        }
    }

    /// The preferences as edited, before validation.
    fn draft(&self, cx: &App) -> WorkspacePreferences {
        let mut next = self.store.read(cx).workspace.preferences.clone();
        next.default_method = selected(&self.method, cx);
        if next.default_method.is_empty() {
            next.default_method = self.base.default_method.clone();
        }
        next.default_body_mode = BodyMode::from_id(&selected(&self.body_mode, cx))
            .unwrap_or(self.base.default_body_mode);
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

    /// Apply valid fields independently. Keep the last valid value of unfinished input.
    fn apply_preferences(&mut self, cx: &mut Context<Self>) {
        let mut next = self.draft(cx);
        let saved = &self.store.read(cx).workspace.preferences;
        for field in relevant_errors(&next.transport).keys() {
            set_field(&mut next.transport, *field, field.get(&saved.transport));
        }
        // Timeout and connect timeout are dependent. Keep both if the resulting
        // pair is still invalid after replacing an unfinished field.
        if !relevant_errors(&next.transport).is_empty() {
            next.transport.timeout_seconds = saved.transport.timeout_seconds;
            next.transport.connect_timeout_seconds = saved.transport.connect_timeout_seconds;
        }
        if !proxy_url_error(&next.transport.proxy_url).is_empty() {
            next.transport.proxy_url = saved.transport.proxy_url.clone();
        }
        if let Some(next) = validated(next) {
            self.store.update(cx, |store, cx| {
                store.update_workspace(cx, |workspace| workspace.set_preferences(next));
            });
        }
        cx.notify();
    }

    fn apply_tokens(&mut self, cx: &mut Context<Self>) {
        let workspace = &self.store.read(cx).workspace;
        let saved_response_tokens = workspace.global_response_tokens.clone();
        let sessions: Vec<_> = workspace
            .sessions
            .iter()
            .filter(|s| workspace.can_move_request(s.id, None))
            .cloned()
            .collect();
        let saved_definitions = workspace.global_definitions.clone();
        let saved_names: Vec<_> = saved_definitions.keys().map(String::as_str).collect();
        let saved_response_tokens = self.response_tokens.update(cx, |editor, cx| {
            editor.valid_changes(&saved_response_tokens, &saved_names, &sessions, cx)
        });
        let reserved: Vec<_> = saved_response_tokens
            .iter()
            .map(|token| token.name.as_str())
            .collect();
        self.token_error.clear();
        self.applied_token_rows
            .retain(|saved| self.token_rows.iter().any(|row| row.id == saved.id));
        // A later rename can free the name of an earlier pending row. Each
        // accepted row moves only to its requested value, so this converges.
        for _ in 0..=self.token_rows.len() {
            let before = self.applied_token_rows.clone();
            self.token_error.clear();
            for row in &self.token_rows {
                let mut next = self.applied_token_rows.clone();
                if let Some(index) = next.iter().position(|saved| saved.id == row.id) {
                    next[index] = row.clone();
                } else {
                    next.push(row.clone());
                }
                let result = rows_to_definitions(&next).and_then(|definitions| {
                    if let Some(name) = definitions
                        .keys()
                        .find(|name| reserved.contains(&name.as_str()))
                    {
                        Err(format!("Another token is named \"{name}\"."))
                    } else {
                        Ok(definitions)
                    }
                });
                match result {
                    Ok(_) => self.applied_token_rows = next,
                    Err(error) => self.token_error = error,
                }
            }
            if self.applied_token_rows == before {
                break;
            }
        }
        let definitions = rows_to_definitions(&self.applied_token_rows).unwrap_or_default();
        let text_names: Vec<_> = definitions.keys().map(String::as_str).collect();
        let response_tokens = self.response_tokens.update(cx, |editor, cx| {
            editor.valid_changes(&saved_response_tokens, &text_names, &sessions, cx)
        });
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                workspace.set_global_definitions(definitions);
                workspace.set_global_response_tokens(response_tokens);
            });
        });
        cx.notify();
    }

    /// Show the cookie jar in the main window.
    fn manage_cookies(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme.update(cx, |theme, cx| theme.cancel_preview(cx));
        let main = cx.try_global::<MainWindow>().map(|main| main.0);
        close(window, cx);
        if let Some(main) = main {
            cx.defer(move |cx| {
                main.update(cx, |_, window, cx| {
                    window.activate_window();
                    window.dispatch_action(Box::new(crate::actions::ManageCookies), cx);
                })
                .ok();
            });
        }
    }

    fn render_number(
        &self,
        index: usize,
        errors: &TransportFieldErrors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (field, _, label, help) = NUMBER_FIELDS[index];
        let colors = theme::colors(cx);
        let input = &self.numbers[index].1;
        let disabled = field == TransportField::MaxRedirects && !self.follow_redirects;
        let description = match (errors.get(&field).cloned(), help) {
            (Some(error), _) => Some(note(error, colors.destructive)),
            (None, Some(help)) => Some(note(help, colors.muted_foreground)),
            _ => None,
        };
        let invalid = errors.contains_key(&field);
        layout::row(
            label,
            description,
            layout::field(
                Input::new(input)
                    .font_family(theme::MONO)
                    .text_size(u(12.))
                    .disabled(disabled)
                    .when(invalid, |this| this.border_color(colors.destructive)),
            ),
            cx,
        )
    }

    fn toggle(
        &self,
        id: &'static str,
        checked: bool,
        set: fn(&mut Self, bool),
        cx: &mut Context<Self>,
    ) -> Switch {
        Switch::new(id)
            .checked(checked)
            .small()
            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                set(this, *checked);
                this.apply_preferences(cx);
            }))
    }

    fn render_general(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let muted = theme::colors(cx).muted_foreground;
        let select = |state: &Entity<ChoiceSelect>| {
            layout::field(
                Select::new(state)
                    .font_family(theme::MONO)
                    .text_size(u(12.)),
            )
        };
        vec![
            layout::group(
                section_heading("Workspace", cx),
                vec![layout::row(
                    "Confirm deletes",
                    Some(note("Ask before deleting a request with content.", muted)),
                    self.toggle(
                        "app-confirm-close",
                        self.confirm_close_drafts,
                        |this, on| this.confirm_close_drafts = on,
                        cx,
                    ),
                    cx,
                )],
                cx,
            ),
            layout::group(
                section_heading_with_help(
                    "defaults-help",
                    "New requests",
                    "These settings apply only when you create a new request. They do not change existing requests or duplicates.",
                    cx,
                ),
                vec![
                    layout::row("Method", None, select(&self.method), cx),
                    layout::row("Body mode", None, select(&self.body_mode), cx),
                ],
                cx,
            ),
            layout::group(
                section_heading("Responses", cx),
                vec![
                    layout::row(
                        "Format responses",
                        None,
                        self.toggle("app-pretty", self.pretty, |this, on| this.pretty = on, cx),
                        cx,
                    ),
                    layout::row(
                        "Wrap lines",
                        None,
                        self.toggle("app-wrap", self.wrap, |this, on| this.wrap = on, cx),
                        cx,
                    ),
                ],
                cx,
            ),
        ]
    }

    fn render_network(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = theme::colors(cx);
        let muted = colors.muted_foreground;
        let draft = self.draft(cx);
        let errors = relevant_errors(&draft.transport);
        let proxy_error = proxy_url_error(&draft.transport.proxy_url);
        let mut numbers: Vec<_> = (0..NUMBER_FIELDS.len())
            .map(|index| self.render_number(index, &errors, cx))
            .collect();
        let inspection = numbers.pop().expect("inspection limit field");
        let max_redirects = numbers.pop().expect("max redirects field");
        let proxy_note = if proxy_error.is_empty() {
            note(
                "http, https, or socks5. Empty uses the system proxy settings.",
                muted,
            )
        } else {
            note(proxy_error.clone(), colors.destructive)
        };
        let tls_note = if self.verify_tls {
            note(
                "Blink trusts the system certificate store, including company and local development CAs.",
                muted,
            )
        } else {
            note(
                "Blink accepts invalid and self-signed certificates for every request. Turn this on again when you finish.",
                colors.warning,
            )
        };
        vec![
            layout::group(section_heading("Timeouts", cx), numbers, cx),
            layout::group(
                section_heading("Redirects", cx),
                vec![
                    layout::row(
                        "Follow redirects",
                        None,
                        self.toggle(
                            "app-follow-redirects",
                            self.follow_redirects,
                            |this, on| this.follow_redirects = on,
                            cx,
                        ),
                        cx,
                    ),
                    max_redirects,
                ],
                cx,
            ),
            layout::group(section_heading("Responses", cx), vec![inspection], cx),
            layout::group(
                section_heading("Connection", cx),
                vec![
                    layout::row(
                        "Proxy URL",
                        Some(proxy_note),
                        layout::field(
                            Input::new(&self.proxy_url)
                                .font_family(theme::MONO)
                                .text_size(u(12.))
                                .when(!proxy_error.is_empty(), |this| {
                                    this.border_color(colors.destructive)
                                }),
                        ),
                        cx,
                    ),
                    layout::row(
                        "Verify TLS certificates",
                        Some(tls_note),
                        self.toggle(
                            "app-verify-tls",
                            self.verify_tls,
                            |this, on| this.verify_tls = on,
                            cx,
                        ),
                        cx,
                    ),
                ],
                cx,
            ),
            layout::group(
                section_heading("Cookies", cx),
                vec![
                    layout::row(
                        "Store and send cookies",
                        Some(note(
                            "One jar for all requests, kept on this device.",
                            muted,
                        )),
                        self.toggle(
                            "app-store-cookies",
                            self.store_cookies,
                            |this, on| this.store_cookies = on,
                            cx,
                        ),
                        cx,
                    ),
                    layout::row(
                        "Cookie jar",
                        None,
                        footer_button("manage-cookies", "Manage cookies…", false).on_click(
                            cx.listener(|this, _, window, cx| this.manage_cookies(window, cx)),
                        ),
                        cx,
                    ),
                ],
                cx,
            ),
        ]
    }

    fn render_tokens(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = theme::colors(cx);
        let editor = v_flex()
            .gap(u(6.))
            .child(
                div()
                    .overflow_hidden()
                    .border_1()
                    .border_color(colors.input)
                    .rounded(u(4.))
                    .bg(colors.background)
                    .child(self.tokens.clone()),
            )
            .when(!self.token_error.is_empty(), |this| {
                this.child(note(self.token_error.clone(), colors.destructive))
            });
        vec![
            layout::group(
                section_heading_with_help(
                    "token-help-global",
                    "Global tokens",
                    "Workspace-global tokens use {{_.name}}. Use {{!NAME}} in a value to read NAME from Blink's process environment when sending.",
                    cx,
                ),
                vec![div().pt(u(6.)).child(editor).into_any_element()],
                cx,
            ),
            self.response_tokens.clone().into_any_element(),
        ]
    }

    fn render_nav(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        v_flex()
            .flex_none()
            .w(u(168.))
            .bg(colors.background)
            .border_1()
            .border_color(colors.border)
            .rounded(px(8.))
            .overflow_hidden()
            .font_family(theme::MONO)
            .child(
                h_flex()
                    .flex_none()
                    .h(u(36.))
                    .pl(u(12.))
                    .border_b_1()
                    .border_color(colors.border)
                    .text_size(u(11.))
                    .font_weight(FontWeight::BOLD)
                    .child(tracked("SETTINGS", 0.08)),
            )
            .child(
                v_flex()
                    .py(u(6.))
                    .children(Page::ALL.into_iter().map(|page| {
                        let active = self.page == page;
                        div()
                            .id(page.title())
                            .debug_selector(move || format!("settings-nav-{}", page.title()))
                            .relative()
                            .flex()
                            .items_center()
                            .h(u(28.))
                            .pl(u(12.))
                            .text_size(u(12.))
                            .cursor_pointer()
                            .text_color(colors.muted_foreground)
                            .when(active, |this| {
                                this.bg(colors.accent).text_color(colors.foreground).child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(2.))
                                        .bg(colors.primary),
                                )
                            })
                            .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
                            .child(page.title())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.theme.update(cx, |theme, cx| theme.end_preview(cx));
                                this.page = page;
                                cx.notify();
                            }))
                    })),
            )
    }
}

impl Render for Settings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let groups = match self.page {
            Page::General => self.render_general(cx),
            Page::Appearance => vec![self.theme.clone().into_any_element()],
            Page::Network => self.render_network(cx),
            Page::Tokens => self.render_tokens(cx),
        };
        let storage_error = self.store.read(cx).error.clone();
        let content = v_flex()
            .flex_1()
            .min_w_0()
            .bg(colors.background)
            .border_1()
            .border_color(colors.border)
            .rounded(px(8.))
            .overflow_hidden()
            .child(layout::page_header(self.page.title(), cx))
            .child(
                div()
                    .id("settings-body")
                    .debug_selector(|| "settings-body".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .p(u(16.))
                            .gap(u(24.))
                            .text_size(u(12.))
                            .when_some(self.page.description(), |this, description| {
                                this.child(note(description, colors.muted_foreground))
                            })
                            .children(groups),
                    ),
            );
        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_stretch()
                    .p(px(6.))
                    .pb_0()
                    .gap(px(6.))
                    .child(self.render_nav(cx))
                    .child(content),
            )
            .child(
                h_flex()
                    .flex_none()
                    .min_h(u(24.))
                    .px(u(14.))
                    .font_family(theme::MONO)
                    .text_size(u(9.))
                    .text_color(colors.muted_foreground)
                    .bg(colors.frame)
                    .map(|this| {
                        if storage_error.is_empty() {
                            this.child(tracked(
                                "CHANGES APPLY IMMEDIATELY · INVALID FIELDS KEEP THEIR LAST VALID VALUE",
                                0.07,
                            ))
                        } else {
                            this.text_color(colors.destructive)
                                .child(tracked(storage_error.to_uppercase(), 0.07))
                        }
                    }),
            )
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
