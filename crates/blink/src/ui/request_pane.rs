//! One request: URL bar, request editor, and response. Port of
//! `RequestWorkspace.vue`, `RequestEditor.vue`, `KeyValueEditor.vue`,
//! `TokenInput.vue`, `ChecksEditor.vue`, and `CodeEditor.vue`.

mod checks;
mod code_panel;
mod common;
mod editor;

use blink_core::codegen::generate_code;
use blink_core::curl_import::{is_curl_command, parse_curl};
use blink_core::environments::request_environment;
use blink_core::model::{CodeTarget, METHODS, PaneLayout};
use blink_core::runner::{PreparedSend, confirm_environment_prompt, prepare_send, socket_active};
use blink_core::websocket_log::is_web_socket_url;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, ThemeStyled as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{CancelRequest, FocusUrl, SendRequest, Unfocus};
use crate::store::{Store, StoreEvent};
use crate::theme;
use crate::ui::response_panel::ResponsePanel;
use crate::ui::token_input::{TokenInput, TokenInputEvent, TokenPaste};
use crate::ui::websocket_panel::WebSocketPanel;
use code_panel::{CloseCode, CodePanel};
use common::{edit_draft, replace_draft, session_context};
use editor::RequestEditor;

/// The shortcut modifier as the Vue app shows it.
const MOD: &str = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" };

/// The notice after a cURL paste.
pub fn import_notice(ignored: &[String]) -> String {
    if ignored.is_empty() {
        "Imported cURL command.".into()
    } else {
        format!("Imported cURL command. Ignored: {}", ignored.join(" "))
    }
}

/// Methods are case-sensitive tokens; the field types them as uppercase.
pub fn normalize_method(value: &str) -> String {
    value.trim().to_uppercase()
}

/// Suggested methods for the text in the method field, as a datalist offers.
pub fn method_suggestions(value: &str) -> Vec<&'static str> {
    let value = value.trim().to_uppercase();
    if METHODS.contains(&value.as_str()) {
        return METHODS.to_vec();
    }
    METHODS
        .into_iter()
        .filter(|method| method.contains(value.as_str()))
        .collect()
}

pub struct RequestPane {
    store: Entity<Store>,
    session_id: u64,
    response: Entity<ResponsePanel>,
    websocket: Option<Entity<WebSocketPanel>>,
    editor: Entity<RequestEditor>,
    code: Entity<CodePanel>,
    url: Entity<TokenInput>,
    method: Entity<InputState>,
    method_focused: bool,
    show_code: bool,
    import_notice: String,
    import_error: String,
    _subscriptions: Vec<Subscription>,
}

impl RequestPane {
    pub fn new(
        store: Entity<Store>,
        session_id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let response = cx.new(|cx| ResponsePanel::new(store.clone(), session_id, window, cx));
        let editor = cx.new(|cx| RequestEditor::new(store.clone(), session_id, window, cx));
        let code = cx.new(|cx| CodePanel::new(store.clone(), window, cx));
        let url = cx.new(|cx| {
            let mut url = TokenInput::new(
                "https://api.example.com/v1/resource or paste a cURL command",
                window,
                cx,
            );
            url.set_appearance(false, cx);
            url.intercept_paste(is_curl_command);
            url
        });
        let method = cx.new(|cx| InputState::new(window, cx));
        let id = session_id;
        let _subscriptions = vec![
            cx.observe_in(&store, window, |this, _, _, cx| {
                this.refresh_url_context(cx);
                cx.notify();
            }),
            cx.subscribe_in(&store, window, move |this, _, event, window, cx| match event {
                StoreEvent::DraftReplaced(replaced) if *replaced == id => this.reload(window, cx),
                StoreEvent::Restored => this.reload(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&url, window, |this, _, event: &TokenInputEvent, window, cx| {
                match event {
                    TokenInputEvent::Change(text) => {
                        let text = text.clone();
                        this.import_error.clear();
                        edit_draft(&this.store, this.session_id, cx, |draft| draft.url = text);
                    }
                    TokenInputEvent::PressEnter => this.primary(window, cx),
                    TokenInputEvent::Blur => {}
                }
            }),
            cx.subscribe_in(&url, window, |this, _, paste: &TokenPaste, _, cx| {
                this.import_curl(&paste.0, cx);
            }),
            cx.subscribe_in(&method, window, |this, input, event, window, cx| match event {
                InputEvent::Change => {
                    let raw = input.read(cx).value().to_string();
                    let method = normalize_method(&raw);
                    if raw != raw.to_uppercase() {
                        let shown = raw.to_uppercase();
                        input.update(cx, |input, cx| input.set_value(shown, window, cx));
                    }
                    edit_draft(&this.store, this.session_id, cx, |draft| draft.method = method);
                }
                InputEvent::Focus => {
                    this.method_focused = true;
                    input.update(cx, |input, cx| input.select_all(window, cx));
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.method_focused = false;
                    cx.notify();
                }
                InputEvent::PressEnter { secondary: false, .. } => this.primary(window, cx),
                InputEvent::PressEnter { .. } => {}
            }),
            cx.subscribe(&code, |this, _, _: &CloseCode, cx| {
                this.show_code = false;
                cx.notify();
            }),
        ];
        let mut pane = RequestPane {
            store,
            session_id,
            response,
            websocket: None,
            editor,
            code,
            url,
            method,
            method_focused: false,
            show_code: false,
            import_notice: String::new(),
            import_error: String::new(),
            _subscriptions,
        };
        pane.reload(window, cx);
        pane
    }

    /// The response side, for the response commands.
    pub fn response(&self) -> &Entity<ResponsePanel> {
        &self.response
    }

    /// Focus the URL field and select its text (`Cmd/Ctrl+L`).
    pub fn focus_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.url.update(cx, |url, cx| url.focus_and_select(window, cx));
    }

    /// Show or hide the code panel.
    pub fn toggle_code(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_code = !self.show_code;
        cx.notify();
    }

    /// The request as code in `target`; empty when the draft does not build.
    pub fn code_for(&self, target: CodeTarget, cx: &App) -> String {
        self.prepared(cx)
            .and_then(|prepared| {
                prepared
                    .http
                    .request()
                    .map(|request| generate_code(target, request, &prepared.options))
            })
            .unwrap_or_default()
    }

    fn prepared(&self, cx: &App) -> Option<PreparedSend> {
        let workspace = &self.store.read(cx).workspace;
        let session = workspace.session(self.session_id)?;
        Some(prepare_send(
            session,
            &workspace.groups,
            &workspace.global_definitions,
            &workspace.preferences,
        ))
    }

    /// Load the URL and method from the draft.
    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.store.read(cx).workspace.session(self.session_id) else {
            return;
        };
        let url = session.draft.url.clone();
        let method = session.draft.method.clone();
        self.refresh_url_context(cx);
        self.url
            .update(cx, |input, cx| input.set_value(&url, window, cx));
        if self.method.read(cx).value().as_ref() != method {
            self.method
                .update(cx, |input, cx| input.set_value(method, window, cx));
        }
        cx.notify();
    }

    fn refresh_url_context(&mut self, cx: &mut Context<Self>) {
        if let Some((_, ctx)) = session_context(self.store.read(cx), self.session_id) {
            let tokens = Some(ctx.tokens);
            self.url
                .update(cx, |input, cx| input.set_context(tokens, cx));
        }
    }

    /// Send, or connect or disconnect a WebSocket.
    fn primary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.session_id;
        self.store.update(cx, |store, cx| store.send(id, window, cx));
    }

    /// Pasting a cURL command into the URL field replaces the draft with it.
    fn import_curl(&mut self, text: &str, cx: &mut Context<Self>) {
        self.import_notice.clear();
        self.import_error.clear();
        match parse_curl(text) {
            Ok(import) => {
                let notice = import_notice(&import.ignored);
                let draft = import.draft;
                replace_draft(&self.store, self.session_id, cx, |current| *current = draft);
                // The URL changed, which clears import errors, not the notice.
                self.import_notice = notice;
            }
            Err(error) => self.import_error = error,
        }
        cx.notify();
    }

    fn set_method(&mut self, method: &str, window: &mut Window, cx: &mut Context<Self>) {
        let method = method.to_string();
        self.method
            .update(cx, |input, cx| input.set_value(method.clone(), window, cx));
        edit_draft(&self.store, self.session_id, cx, |draft| draft.method = method);
        self.method_focused = false;
        self.url.update(cx, |url, cx| url.focus(window, cx));
        cx.notify();
    }

    fn render_method_suggestions(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = theme::colors(cx);
        let input = self.method.read(cx);
        let bounds = input.input_bounds();
        let value = input.value().to_string();
        let rows = method_suggestions(&value).into_iter().map(|method| {
            div()
                .id(SharedString::from(format!("method-option-{method}")))
                .h(px(24.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(3.))
                .font_family(theme::MONO)
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::method_color(method, cx))
                .hover(|this| this.bg(colors.accent))
                .child(method)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.set_method(method, window, cx);
                    }),
                )
        });
        deferred(
            anchored()
                .position(point(bounds.left(), bounds.bottom() + px(4.)))
                .snap_to_window()
                .child(
                    div()
                        .id("method-suggestions")
                        .occlude()
                        .popover_style(cx)
                        .p_1()
                        .min_w(px(120.))
                        .children(rows),
                ),
        )
        .with_priority(1)
    }

    fn render_bar(&self, prepared: &PreparedSend, busy: bool, socket_live: bool, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let websocket = prepared.websocket;
        let can_build = prepared.http.request().is_some();
        let url_text = self
            .store
            .read(cx)
            .workspace
            .session(self.session_id)
            .map(|session| session.draft.url.clone())
            .unwrap_or_default();
        let curl = prepared.http.curl(&prepared.options);
        let show_code = self.show_code;
        let entity = cx.entity().downgrade();
        let store = self.store.clone();
        let kbd = |text: String| {
            div()
                .ml(px(10.))
                .opacity(0.65)
                .text_size(px(10.))
                .child(text)
        };
        let method_field = if websocket {
            div()
                .w(px(94.))
                .flex_none()
                .flex()
                .items_center()
                .border_r_1()
                .border_color(colors.border)
                .px_3()
                .font_family(theme::MONO)
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(colors.info)
                .child("WS")
                .into_any_element()
        } else {
            let method = self.method.read(cx).value().to_string();
            div()
                .w(px(94.))
                .flex_none()
                .flex()
                .items_center()
                .border_r_1()
                .border_color(colors.border)
                .px_1()
                .font_family(theme::MONO)
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::method_color(&normalize_method(&method), cx))
                .child(
                    Input::new(&self.method)
                        .appearance(false)
                        .small()
                        .px(px(8.))
                        .font_family(theme::MONO)
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .when(self.method_focused, |this| {
                    this.child(self.render_method_suggestions(cx))
                })
                .into_any_element()
        };
        let primary_button = if websocket {
            Button::new("connect")
                .when(socket_live, |this| this.secondary())
                .when(!socket_live, |this| this.primary())
                .h(px(34.))
                .px_3()
                .disabled(!socket_live && prepared.socket.is_err())
                .icon(Icon::new(if socket_live { IconName::Unplug } else { IconName::Plug }).size(px(14.)))
                .child(if socket_live { "Disconnect" } else { "Connect" })
                .child(kbd(format!("{MOD} ↵")))
                .on_click(cx.listener(|this, _, window, cx| this.primary(window, cx)))
        } else if busy {
            Button::new("cancel")
                .secondary()
                .h(px(34.))
                .px_3()
                .icon(Icon::new(IconName::Square).size(px(12.)))
                .child("Cancel")
                .child(kbd(format!("{MOD} .")))
                .on_click(cx.listener(|this, _, _, cx| {
                    let id = this.session_id;
                    this.store.update(cx, |store, cx| store.cancel(id, cx));
                }))
        } else {
            Button::new("send")
                .primary()
                .h(px(34.))
                .px_3()
                .disabled(!can_build)
                .icon(Icon::new(IconName::ArrowUpRight).size(px(15.)))
                .child("Send")
                .child(kbd(format!("{MOD} ↵")))
                .on_click(cx.listener(|this, _, window, cx| this.primary(window, cx)))
        };
        div()
            .id("request-bar")
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .px(px(14.))
            .py_3()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.secondary)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(34.))
                    .border_1()
                    .border_color(colors.input)
                    .rounded(px(4.))
                    .bg(colors.background)
                    .when(self.focused_in_bar(window, cx), |this| this.border_color(colors.primary))
                    .child(method_field)
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .px_1()
                            .font_family(theme::MONO)
                            .text_size(px(12.))
                            .child(self.url.clone()),
                    ),
            )
            .child(
                Button::new("code")
                    .secondary()
                    .h(px(34.))
                    .disabled(!can_build || websocket)
                    .icon(Icon::new(IconName::SquareTerminal).size(px(14.)))
                    .label("Code")
                    .tooltip("Show the request as code")
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_code(window, cx))),
            )
            .child(primary_button)
            .context_menu(move |menu, window, cx| {
                let copy_url = url_text.clone();
                let copy_curl = curl.clone();
                let url_store = store.clone();
                let curl_store = store.clone();
                let toggle = entity.clone();
                let code_entity = entity.clone();
                let code_store = store.clone();
                menu.map(|menu| {
                    if busy {
                        menu.menu("Cancel request", Box::new(CancelRequest))
                    } else {
                        menu.menu_with_disabled("Send", Box::new(SendRequest), !can_build)
                    }
                })
                .menu("Focus URL", Box::new(FocusUrl))
                .separator()
                .item(PopupMenuItem::new("Copy URL").disabled(copy_url.is_empty()).on_click(
                    move |_, _, cx| {
                        let text = copy_url.clone();
                        url_store.update(cx, |store, cx| store.copy(text, cx));
                    },
                ))
                .item(PopupMenuItem::new("Copy as cURL").disabled(copy_curl.is_empty()).on_click(
                    move |_, _, cx| {
                        let text = copy_curl.clone();
                        curl_store.update(cx, |store, cx| store.copy(text, cx));
                    },
                ))
                .when(can_build, |menu| {
                    menu.submenu("Copy as", window, cx, move |menu, _, _| {
                        CodeTarget::ALL.into_iter().fold(menu, |menu, target| {
                            let entity = code_entity.clone();
                            let store = code_store.clone();
                            menu.item(PopupMenuItem::new(target.label()).on_click(move |_, _, cx| {
                                let code = entity
                                    .upgrade()
                                    .map(|pane| pane.read(cx).code_for(target, cx))
                                    .unwrap_or_default();
                                store.update(cx, |store, cx| store.copy(code, cx));
                            }))
                        })
                    })
                })
                .when(!can_build, |menu| menu.item(PopupMenuItem::new("Copy as").disabled(true)))
                .item(PopupMenuItem::new("Show code").checked(show_code).on_click(
                    move |_, window, cx| {
                        toggle
                            .update(cx, |this, cx| this.toggle_code(window, cx))
                            .ok();
                    },
                ))
            })
    }

    /// The URL bar border turns amber while its fields have focus.
    fn focused_in_bar(&self, window: &Window, cx: &App) -> bool {
        self.method_focused || self.url.read(cx).state().read(cx).focus_handle(cx).is_focused(window)
    }

    fn render_notices(&self, prepared: &PreparedSend, websocket: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let colors = theme::colors(cx);
        let store = self.store.read(cx);
        let mut notices = Vec::new();
        let line = || {
            div()
                .px(px(14.))
                .py_2()
                .border_b_1()
                .border_color(colors.border)
                .font_family(theme::MONO)
                .text_size(px(11.))
                .line_height(relative(1.6))
        };
        let session = store.workspace.session(self.session_id);
        let environment = session
            .and_then(|session| request_environment(session.group_id, &store.workspace.groups))
            .cloned();
        if store.confirming(self.session_id)
            && let Some(environment) = environment
        {
            let (_, verb) = confirm_environment_prompt(&environment, websocket);
            let color = theme::environment_color(environment.color, cx);
            notices.push(
                line()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color)
                                    .child(environment.name.clone()),
                            )
                            .child(format!(" is protected. {verb} to {}?", environment.name)),
                    )
                    .child(Button::new("confirm-cancel").ghost().small().label("Cancel").on_click(
                        cx.listener(|this, _, _, cx| {
                            let id = this.session_id;
                            this.store.update(cx, |store, cx| store.dismiss_confirm(id, cx));
                        }),
                    ))
                    .child(Button::new("confirm-environment").primary().small().label(verb).on_click(
                        cx.listener(|this, _, window, cx| {
                            let id = this.session_id;
                            this.store
                                .update(cx, |store, cx| store.confirm_and_send(id, window, cx));
                        }),
                    ))
                    .into_any_element(),
            );
        }
        if !self.import_error.is_empty() {
            notices.push(
                line()
                    .text_color(colors.destructive)
                    .child(self.import_error.clone())
                    .into_any_element(),
            );
        } else if !self.import_notice.is_empty() {
            notices.push(
                line()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(colors.muted_foreground)
                    .child(div().flex_1().min_w_0().child(self.import_notice.clone()))
                    .child(
                        Button::new("dismiss-import")
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::X).size(px(12.)))
                            .tooltip("Dismiss import notice")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.import_notice.clear();
                                cx.notify();
                            })),
                    )
                    .into_any_element(),
            );
        }
        let has_url = session.is_some_and(|session| !session.draft.url.is_empty());
        let error = prepared.validation_error();
        if has_url && !error.is_empty() {
            notices.push(
                line()
                    .text_color(colors.destructive)
                    .child(error.to_string())
                    .into_any_element(),
            );
        }
        notices
    }
}

impl Render for RequestPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let Some(prepared) = self.prepared(cx) else {
            return div().size_full().into_any_element();
        };
        let store = self.store.read(cx);
        let workspace = &store.workspace;
        let Some(session) = workspace.session(self.session_id) else {
            return div().size_full().into_any_element();
        };
        let busy = session.busy;
        let socket_live = socket_active(session);
        let stacked = workspace.preferences.pane_layout == PaneLayout::Vertical;
        let target = workspace.preferences.code_target;
        let websocket = is_web_socket_url(&session.draft.url);

        if self.show_code && !websocket {
            let code = prepared
                .http
                .request()
                .map(|request| generate_code(target, request, &prepared.options))
                .unwrap_or_default();
            self.code
                .update(cx, |panel, cx| panel.show(target, &code, window, cx));
        }
        let response: AnyElement = if websocket {
            let store = self.store.clone();
            let id = self.session_id;
            self.websocket
                .get_or_insert_with(|| cx.new(|cx| WebSocketPanel::new(store, id, window, cx)))
                .clone()
                .into_any_element()
        } else {
            self.response.clone().into_any_element()
        };
        let split_id = format!("request-split-{}-{}", self.session_id, if stacked { "v" } else { "h" });
        let panels = if stacked {
            v_resizable(SharedString::from(split_id))
                .child(
                    resizable_panel()
                        .size(px(280.))
                        .size_range(px(160.)..Pixels::MAX)
                        .child(self.editor.clone()),
                )
                .child(
                    resizable_panel()
                        .size_range(px(200.)..Pixels::MAX)
                        .child(response),
                )
        } else {
            h_resizable(SharedString::from(split_id))
                .child(
                    resizable_panel()
                        .size(px(420.))
                        .size_range(px(280.)..Pixels::MAX)
                        .child(self.editor.clone()),
                )
                .child(
                    resizable_panel()
                        .size_range(px(340.)..Pixels::MAX)
                        .child(response),
                )
        };
        let notices = self.render_notices(&prepared, websocket, cx);
        let show_code = self.show_code && !websocket;
        div()
            .id(SharedString::from(format!("request-pane-{}", self.session_id)))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .bg(colors.background)
            .on_action(cx.listener(|this, _: &Unfocus, _, cx| {
                if this.show_code {
                    this.show_code = false;
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .child(self.render_bar(&prepared, busy, socket_live, window, cx))
            .children(notices)
            .when(show_code, |this| this.child(self.code.clone()))
            .child(div().flex_1().min_h_0().child(panels))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn notes_ignored_curl_options() {
        assert_eq!(import_notice(&[]), "Imported cURL command.");
        assert_eq!(
            import_notice(&["--compressed".into(), "-v".into()]),
            "Imported cURL command. Ignored: --compressed -v"
        );
    }

    #[test]
    fn types_methods_as_uppercase_tokens() {
        assert_eq!(normalize_method(" patch "), "PATCH");
        assert_eq!(normalize_method("purge"), "PURGE");
    }

    #[test]
    fn suggests_methods_that_match_the_typed_text() {
        assert_eq!(method_suggestions("GET").len(), METHODS.len());
        assert_eq!(method_suggestions("p"), ["POST", "PUT", "PATCH", "OPTIONS"]);
        assert!(method_suggestions("PURGE").is_empty());
    }
}
