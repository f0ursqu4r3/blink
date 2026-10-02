//! A single-line text field that shows `{{token}}` references resolved and
//! suggests token names. Port of `TokenInput.vue`.
//!
//! The owner sets the value and the token context; user edits arrive as
//! `TokenInputEvent::Change` and are never echoed back by `set_value`.
//!
//! Each reference is an atomic inline token of the input: a defined token
//! shows its value, an undefined or environment reference shows its text in
//! a warning color. The input text stays the raw text, so copy and cut keep
//! the references. Backspace at the end of a reference (or Delete at its
//! start) removes one raw character, so the reference turns back into text
//! the user can edit.
//!
//! A secret field (`set_secret`) labels every reference with its raw text.
//! While it is not focused, a row over the input shows its plain text as
//! bullets and its references as typed, so a mistyped name stays visible.

use std::ops::Range;

use blink_core::interpolation::InterpolationContext;
use blink_core::token_display::token_display;
use blink_core::token_hints::{
    TokenOption, TokenScope, TokenState, apply_token_option, match_token_options, token_hint,
    token_options, token_query_at, token_spans,
};
use gpui_kit::component::input::{
    Backspace, Delete, Enter, Escape, IndentInline, InlineToken, InlineTokenContext, Input,
    InputContent, InputEvent, InputState, MoveDown, MoveUp,
};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, ThemeStyled as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;

#[derive(Debug, Clone)]
pub enum TokenInputEvent {
    /// The user changed the raw text.
    Change(String),
    /// Plain Enter, with no suggestion open. `Cmd/Ctrl+Enter` propagates to
    /// the app instead.
    PressEnter,
    Blur,
}

/// The user pasted text into a field that intercepts pastes (see
/// `TokenInput::intercept_paste`). The text was not inserted.
#[derive(Debug, Clone)]
pub struct TokenPaste(pub String);

/// Decides whether a paste is taken over by the owner.
type PasteFilter = Box<dyn Fn(&str) -> bool>;

pub struct TokenInput {
    input: Entity<InputState>,
    context: Option<InterpolationContext>,
    /// The raw text last reported or set, so re-tokenizing never re-emits.
    last_value: String,
    /// Highlighted suggestion.
    active: usize,
    /// Escape closed the suggestions for the reference starting here.
    dismissed: Option<usize>,
    focused: bool,
    retokenize: bool,
    paste_filter: Option<std::rc::Rc<PasteFilter>>,
    size: Option<gpui_kit::component::Size>,
    appearance: bool,
    text_size: Pixels,
    disabled: bool,
    muted: bool,
    secret: bool,
    height: Option<Pixels>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TokenInputEvent> for TokenInput {}
impl EventEmitter<TokenPaste> for TokenInput {}

impl TokenInput {
    pub fn new(
        placeholder: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.into();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let _subscriptions = vec![
            cx.subscribe_in(&input, window, Self::on_input_event),
            // Cursor moves re-render the suggestions.
            cx.observe(&input, |_, _, cx| cx.notify()),
        ];
        TokenInput {
            input,
            context: None,
            last_value: String::new(),
            active: 0,
            dismissed: None,
            focused: false,
            retokenize: false,
            paste_filter: None,
            size: None,
            appearance: true,
            text_size: px(12.),
            disabled: false,
            muted: false,
            secret: false,
            height: None,
            _subscriptions,
        }
    }

    /// Replace the text without emitting `Change`.
    pub fn set_value(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.last_value = value.to_string();
        let content = token_content(value, self.context.as_ref(), self.secret);
        self.input
            .update(cx, |input, cx| input.set_value(content, window, cx));
    }

    pub fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Tokens for resolving and suggesting `{{name}}` references.
    pub fn set_context(&mut self, context: Option<InterpolationContext>, cx: &mut Context<Self>) {
        if self.context == context {
            return;
        }
        self.context = context;
        self.retokenize_all(cx);
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// Focus the field and select its text.
    pub fn focus_and_select(&self, window: &mut Window, cx: &mut App) {
        self.focus(window, cx);
        self.input
            .update(cx, |input, cx| input.select_all(window, cx));
    }

    /// The input state, for owners that need its focus handle.
    pub fn state(&self) -> &Entity<InputState> {
        &self.input
    }

    /// Take over pastes whose text passes `filter`: they emit `TokenPaste`
    /// and insert nothing. The URL field uses it for cURL commands.
    pub fn intercept_paste(&mut self, filter: impl Fn(&str) -> bool + 'static) {
        self.paste_filter = Some(std::rc::Rc::new(Box::new(filter)));
    }

    /// Draw the field without its own border and background, for table cells
    /// and the URL bar.
    pub fn set_appearance(&mut self, appearance: bool, cx: &mut Context<Self>) {
        self.appearance = appearance;
        cx.notify();
    }

    pub fn set_size(&mut self, size: gpui_kit::component::Size, cx: &mut Context<Self>) {
        self.size = Some(size);
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        cx.notify();
    }

    /// Show the text in the muted color, as a disabled key/value row.
    pub fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        self.muted = muted;
        cx.notify();
    }

    /// Mask the text of a credential field. References always show as
    /// typed, never as their values.
    pub fn set_secret(&mut self, secret: bool, cx: &mut Context<Self>) {
        if self.secret == secret {
            return;
        }
        self.secret = secret;
        self.retokenize_all(cx);
        cx.notify();
    }

    /// A fixed field height, for forms with taller rows.
    pub fn set_height(&mut self, height: Pixels, cx: &mut Context<Self>) {
        self.height = Some(height);
        cx.notify();
    }

    /// The masked row a secret field shows while it is not focused, or
    /// `None` when the field shows its text.
    pub fn secret_segments(&self) -> Option<Vec<(String, Option<TokenState>)>> {
        (self.secret && !self.focused)
            .then(|| secret_display(&self.last_value, self.context.as_ref()))
    }

    fn on_input_event(
        &mut self,
        input: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let value = input.read(cx).value().to_string();
                if value == self.last_value {
                    // Tokens only: an undo of a tokenization, or the
                    // tokenization itself. Keep it as it is.
                    return;
                }
                self.last_value = value.clone();
                self.tokenize_new(window, cx);
                cx.emit(TokenInputEvent::Change(value));
                cx.notify();
            }
            InputEvent::PressEnter { secondary, .. } => {
                if !secondary {
                    cx.emit(TokenInputEvent::PressEnter);
                }
            }
            InputEvent::Focus => {
                self.focused = true;
                cx.notify();
            }
            InputEvent::Blur => {
                self.focused = false;
                self.dismissed = None;
                if self.secret {
                    // The masked row starts at the first character; so does
                    // the text under it, for the next click.
                    self.input.update(cx, |input, cx| {
                        input.set_scroll_offset(point(px(0.), px(0.)), cx)
                    });
                }
                cx.emit(TokenInputEvent::Blur);
                cx.notify();
            }
        }
    }

    /// Turn references the user just completed into tokens. Existing tokens
    /// stay; the selection is kept.
    fn tokenize_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let wanted = token_ranges(&self.last_value, self.context.as_ref(), self.secret);
        self.input.update(cx, |input, cx| {
            let existing: Vec<Range<usize>> = input.tokens().iter().map(|s| s.range()).collect();
            let missing: Vec<_> = wanted
                .into_iter()
                .filter(|(range, _)| !existing.contains(range))
                .collect();
            if missing.is_empty() {
                return;
            }
            let selection = input.selected_range();
            // Same-length replacements keep every other range valid.
            for (range, token) in missing {
                let _ = input.replace_range_with_token(range, token, window, cx);
            }
            input.set_selected_range(selection, cx);
        });
    }

    /// Rebuild every token for a new context on the next render, which has
    /// the window. Clears the field's undo history.
    fn retokenize_all(&mut self, _cx: &mut Context<Self>) {
        self.retokenize = true;
    }

    fn apply_retokenize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.retokenize) {
            return;
        }
        let content = token_content(&self.last_value, self.context.as_ref(), self.secret);
        self.input.update(cx, |input, cx| {
            if input.value().as_ref() != content.text().as_ref()
                || input.tokens() == content.tokens()
            {
                return;
            }
            let selection = input.selected_range();
            input.set_value(content, window, cx);
            input.set_selected_range(selection, cx);
        });
    }

    /// Replace `range` of the raw text with `text`, as one undoable edit.
    fn replace_raw(
        &mut self,
        range: Range<usize>,
        text: &str,
        caret: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = text.to_string();
        self.input.update(cx, |input, cx| {
            input.set_selected_range(range, cx);
            input.replace(text, window, cx);
            input.set_selected_range(caret..caret, cx);
        });
    }

    /// The suggestions for the reference being typed, and where it starts.
    fn suggestions(&self, cx: &App) -> Option<(usize, Vec<TokenOption>)> {
        if !self.focused || self.disabled {
            return None;
        }
        let options = token_options(self.context.as_ref());
        if options.is_empty() {
            return None;
        }
        let input = self.input.read(cx);
        let selection = input.selected_range();
        if !selection.is_empty() {
            return None;
        }
        let query = token_query_at(&self.last_value, selection.start)?;
        if self.dismissed == Some(query.from) {
            return None;
        }
        let matches = match_token_options(&options, &query.query);
        (!matches.is_empty()).then_some((query.from, matches))
    }

    fn accept(&mut self, option: &TokenOption, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.input.read(cx).selected_range().start;
        let Some(query) = token_query_at(&self.last_value, cursor) else {
            return;
        };
        let edit = apply_token_option(&self.last_value, query.from, cursor, &option.name);
        // The option replaces the typed query and a `}}` after the cursor.
        let replaced_end = self.last_value.len() - (edit.text.len() - edit.cursor);
        let inserted = edit.text[query.from..edit.cursor].to_string();
        self.replace_raw(query.from..replaced_end, &inserted, edit.cursor, window, cx);
        self.active = 0;
    }

    fn on_backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_into_token(true, window, cx);
    }

    fn on_delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_into_token(false, window, cx);
    }

    /// Backspace just after a token, or Delete just before one: remove one
    /// raw character so the reference shows as text again.
    fn delete_into_token(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.input.read(cx);
        let selection = input.selected_range();
        if !selection.is_empty() {
            return;
        }
        let at = selection.start;
        let Some(span) = input
            .tokens()
            .iter()
            .find(|span| {
                if backward {
                    span.range().end == at
                } else {
                    span.range().start == at
                }
            })
            .cloned()
        else {
            return;
        };
        cx.stop_propagation();
        let range = span.range();
        let raw = span.token().text().to_string();
        // A reference starts with `{{` and ends with `}}`: one byte each.
        let (text, caret) = if backward {
            (raw[..raw.len() - 1].to_string(), range.end - 1)
        } else {
            (raw[1..].to_string(), range.start)
        };
        self.replace_raw(range, &text, caret, window, cx);
    }

    /// Keys for the open suggestions. Returns whether one was open.
    fn suggestion_key(
        &mut self,
        key: SuggestionKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((from, matches)) = self.suggestions(cx) else {
            return false;
        };
        let count = matches.len();
        let active = self.active.min(count - 1);
        match key {
            SuggestionKey::Down => self.active = (active + 1) % count,
            SuggestionKey::Up => self.active = (active + count - 1) % count,
            SuggestionKey::Accept => {
                let option = matches[active].clone();
                self.accept(&option, window, cx);
            }
            SuggestionKey::Close => self.dismissed = Some(from),
        }
        cx.stop_propagation();
        cx.notify();
        true
    }

    /// The masked text, over the input's text area. It has no listeners, so
    /// a click reaches the input and focuses it.
    fn render_secret_row(
        &self,
        segments: Vec<(String, Option<TokenState>)>,
        colors: theme::Colors,
    ) -> Div {
        let size = self.size.unwrap_or_default();
        // The input's border, then its padding.
        let border = if self.appearance { px(1.) } else { px(0.) };
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(border + size.input_px())
            .right(border + size.input_px())
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .font_family(theme::MONO)
            .text_size(self.text_size)
            .when(self.disabled, |this| this.opacity(0.5))
            .children(segments.into_iter().map(|(text, state)| {
                match state {
                    Some(state) => reference_style(div().flex_none(), state, text.into(), colors),
                    None => div()
                        .flex_none()
                        .text_color(if self.muted {
                            colors.muted_foreground
                        } else {
                            colors.foreground
                        })
                        .child(text),
                }
            }))
    }

    fn render_suggestions(
        &mut self,
        from: usize,
        matches: Vec<TokenOption>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = theme::colors(cx);
        let input = self.input.read(cx);
        let bounds = input.input_bounds();
        let left = input
            .range_to_bounds(&(from.saturating_sub(2)..from.saturating_sub(2)))
            .map_or(bounds.left(), |b| b.left())
            .max(bounds.left())
            .min(bounds.right());
        let position = point(left, bounds.bottom() + px(2.));
        let active = self.active.min(matches.len().saturating_sub(1));
        let secret = self.secret;
        let _ = window;
        let rows = matches.into_iter().enumerate().map(|(index, option)| {
            let selected = index == active;
            let scope = match option.scope {
                TokenScope::Local => "Group",
                TokenScope::Global => "Global",
            };
            let accept = option.clone();
            div()
                .id(("token-option", index))
                .h(px(24.))
                .px_2()
                .flex()
                .items_center()
                .gap_4()
                .rounded(px(3.))
                .cursor_default()
                .when(selected, |this| {
                    this.bg(colors.accent).text_color(colors.foreground)
                })
                .child(
                    div()
                        .flex_none()
                        .font_family(theme::MONO)
                        .text_color(colors.keyword)
                        .child(option.name.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_family(theme::MONO)
                        .text_color(colors.muted_foreground)
                        .children(suggestion_value(&option, secret)),
                )
                .child(
                    div()
                        .flex_none()
                        .font_family(theme::SANS)
                        .text_color(colors.muted_foreground)
                        .child(scope),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.accept(&accept, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(move |this, _, _, cx| {
                    if this.active != index {
                        this.active = index;
                        cx.notify();
                    }
                }))
        });
        deferred(
            anchored().position(position).snap_to_window().child(
                div()
                    .id("token-suggestions")
                    .occlude()
                    .popover_style(cx)
                    .p_1()
                    .min_w(px(180.))
                    .max_w(px(480.))
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .text_size(px(12.))
                    .children(rows),
            ),
        )
        .with_priority(1)
    }
}

#[derive(Clone, Copy)]
enum SuggestionKey {
    Down,
    Up,
    Accept,
    Close,
}

/// The token id encodes its state, for the renderer.
fn state_id(state: TokenState) -> &'static str {
    match state {
        TokenState::Resolved => "resolved",
        TokenState::Unresolved => "unresolved",
        TokenState::Env => "env",
        TokenState::Response => "response",
    }
}

fn state_from_id(id: &str) -> TokenState {
    match id.split(':').next().unwrap_or_default() {
        "resolved" => TokenState::Resolved,
        "env" => TokenState::Env,
        "response" => TokenState::Response,
        _ => TokenState::Unresolved,
    }
}

/// A reference in its state's colors: a value on a tint, or a warning.
fn reference_style<E: Styled + ParentElement>(
    this: E,
    state: TokenState,
    label: SharedString,
    colors: theme::Colors,
) -> E {
    match state {
        TokenState::Resolved => this.text_color(colors.keyword).child(
            div()
                .rounded(px(2.))
                .bg(colors.keyword.opacity(0.15))
                .child(label),
        ),
        TokenState::Response => this.text_color(colors.info).child(
            div()
                .rounded(px(2.))
                .bg(colors.info.opacity(0.15))
                .child(label),
        ),
        TokenState::Env => this.text_color(colors.warning).child(label),
        TokenState::Unresolved => this
            .text_color(colors.destructive)
            .text_decoration_1()
            .text_decoration_wavy()
            .text_decoration_color(colors.destructive.opacity(0.6))
            .child(label),
    }
}

/// A label the input accepts: no control characters, never empty.
fn clean_label(text: &str, fallback: &str) -> String {
    let label: String = text
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
                ' '
            } else {
                c
            }
        })
        .collect();
    if label.trim().is_empty() {
        fallback.to_string()
    } else {
        label
    }
}

/// Every reference in `raw` as a token over its raw text. A defined token is
/// labeled with its value, unless `raw_labels` asks for the raw text.
fn token_ranges(
    raw: &str,
    ctx: Option<&InterpolationContext>,
    raw_labels: bool,
) -> Vec<(Range<usize>, InlineToken)> {
    token_display(raw, ctx)
        .segments
        .into_iter()
        .filter_map(|segment| {
            let state = segment.span.token?;
            let text = segment.span.text.clone();
            let label = if segment.unit && !raw_labels {
                clean_label(&segment.text, &text)
            } else {
                text.clone()
            };
            let token =
                InlineToken::new(format!("{}:{}", state_id(state), text), text).with_label(label);
            Some((segment.raw_from..segment.raw_to, token))
        })
        .collect()
}

/// The raw text with its references as tokens.
fn token_content(raw: &str, ctx: Option<&InterpolationContext>, raw_labels: bool) -> InputContent {
    token_ranges(raw, ctx, raw_labels).into_iter().fold(
        InputContent::new(raw.to_string()),
        |content, (range, token)| {
            let fallback = content.clone();
            content.with_token(range, token).unwrap_or(fallback)
        },
    )
}

/// A secret field's text as shown while it is not focused: each plain
/// character as a bullet, each reference as typed with its state.
pub fn secret_display(
    raw: &str,
    ctx: Option<&InterpolationContext>,
) -> Vec<(String, Option<TokenState>)> {
    token_spans(raw, ctx)
        .into_iter()
        .filter(|span| !span.text.is_empty())
        .map(|span| match span.token {
            Some(state) => (span.text, Some(state)),
            None => ("\u{2022}".repeat(span.text.chars().count()), None),
        })
        .collect()
}

/// The hover hint for one reference.
/// The value column of a suggestion. A secret field never shows values.
fn suggestion_value(option: &TokenOption, secret: bool) -> Option<String> {
    (!secret).then(|| option.value.clone())
}

/// A `secret` field never shows a token's value: a defined token reads
/// `name is defined`.
fn reference_hint(
    raw: &str,
    ctx: Option<&InterpolationContext>,
    secret: bool,
    now_ms: f64,
) -> String {
    let Some(span) = token_spans(raw, ctx).into_iter().next() else {
        return String::new();
    };
    match (&span.token, &span.name) {
        (Some(TokenState::Resolved), Some(name)) if secret => format!("{name} is defined"),
        _ => token_hint(&span, ctx, now_ms),
    }
}

/// One token as the input draws it: a value on a tint, or a warning. It
/// shows as selected only in a `focused` field, as the selected text does.
fn render_token(
    token: &InlineTokenContext,
    ctx: Option<&InterpolationContext>,
    focused: bool,
    secret: bool,
    masked: bool,
    cx: &App,
) -> AnyElement {
    let colors = theme::colors(cx);
    let id = token.token().id().to_string();
    let raw = token.token().text().to_string();
    let hint = reference_hint(&raw, ctx, secret, blink_core::history::now_ms());
    let label = token.token().label().clone();
    div()
        .id(SharedString::from(format!("token-{}", token.range().start)))
        .flex()
        .items_center()
        .h(token.line_height())
        .max_w(token.available_width())
        .overflow_hidden()
        .whitespace_nowrap()
        .map(|this| reference_style(this, state_from_id(&id), label, colors))
        .when(masked, |this| this.opacity(0.))
        .when(focused && token.is_selected(), |this| {
            this.bg(colors.selection)
        })
        .when(!hint.is_empty() && !masked, |this| {
            let hint = hint.clone();
            this.tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
        })
        .into_any_element()
}

impl Render for TokenInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_retokenize(window, cx);
        let suggestions = self.suggestions(cx);
        let ctx = self.context.clone();
        let secret = self.secret;
        let colors = theme::colors(cx);
        let paste_filter = self.paste_filter.clone();
        let this = cx.entity().downgrade();
        let focus = self.input.read(cx).focus_handle(cx);
        // The input stays drawn under the masked row, so it keeps its size,
        // takes the click, and focuses as usual.
        let secret_segments = self.secret_segments().filter(|_| !focus.is_focused(window));
        let masked = secret_segments.is_some();
        let mut input = Input::new(&self.input)
            .font_family(theme::MONO)
            .text_size(self.text_size)
            .appearance(self.appearance)
            .disabled(self.disabled)
            .hide_accessibility_value(self.secret)
            .when(masked, |input| {
                input.text_color(colors.foreground.opacity(0.))
            })
            .token(move |token, window, cx| {
                render_token(
                    token,
                    ctx.as_ref(),
                    focus.is_focused(window),
                    secret,
                    masked,
                    cx,
                )
            });
        if let Some(size) = self.size {
            input = input.with_size(size);
        }
        if let Some(height) = self.height {
            input = input.h(height);
        }
        let secret_row = secret_segments.map(|segments| self.render_secret_row(segments, colors));
        if let Some(filter) = paste_filter {
            input = input.on_paste(move |item, _, cx| {
                let Some(text) = item.text() else {
                    return false;
                };
                if !filter(&text) {
                    return false;
                }
                this.update(cx, |_, cx| cx.emit(TokenPaste(text))).is_ok()
            });
        }
        div()
            .flex_1()
            .min_w_0()
            .relative()
            .when(self.muted, |this| this.text_color(colors.muted_foreground))
            .capture_action(cx.listener(Self::on_backspace))
            .capture_action(cx.listener(Self::on_delete))
            .capture_action(cx.listener(|this, _: &MoveDown, window, cx| {
                this.suggestion_key(SuggestionKey::Down, window, cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveUp, window, cx| {
                this.suggestion_key(SuggestionKey::Up, window, cx);
            }))
            .capture_action(cx.listener(|this, action: &Enter, window, cx| {
                if !action.secondary {
                    this.suggestion_key(SuggestionKey::Accept, window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &IndentInline, window, cx| {
                this.suggestion_key(SuggestionKey::Accept, window, cx);
            }))
            .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                this.suggestion_key(SuggestionKey::Close, window, cx);
            }))
            .child(input)
            .children(secret_row)
            .when_some(suggestions, |this, (from, matches)| {
                this.child(self.render_suggestions(from, matches, window, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blink_core::model::Definitions;
    use core::prelude::v1::test;

    fn ctx() -> InterpolationContext {
        let defs = |entries: &[(&str, &str)]| -> Definitions {
            entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        InterpolationContext::new(
            defs(&[("endpoint", "users")]),
            defs(&[("host", "api.test")]),
        )
    }

    #[test]
    fn shows_a_defined_token_as_its_value_and_keeps_the_raw_text() {
        let content = token_content("https://{{host}}/{{endpoint}}", Some(&ctx()), false);
        assert_eq!(content.text().as_ref(), "https://{{host}}/{{endpoint}}");
        let labels: Vec<_> = content
            .tokens()
            .iter()
            .map(|span| span.token().label().to_string())
            .collect();
        assert_eq!(labels, ["api.test", "users"]);
    }

    #[test]
    fn colors_resolved_and_unresolved_references() {
        let ids: Vec<_> = token_ranges("{{host}}{{nope}}{{!HOME}}", Some(&ctx()), false)
            .into_iter()
            .map(|(_, token)| token.id().to_string())
            .collect();
        assert_eq!(
            ids,
            ["resolved:{{host}}", "unresolved:{{nope}}", "env:{{!HOME}}"]
        );
    }

    #[test]
    fn has_no_tokens_for_text_without_references() {
        assert!(
            token_content("https://example.com", Some(&ctx()), false)
                .tokens()
                .is_empty()
        );
    }

    #[test]
    fn secret_display_masks_text_and_shows_references() {
        let segments = secret_display("ab{{host}}c{{nope}}", Some(&ctx()));
        let text: String = segments.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(text, "••{{host}}•{{nope}}");
        assert_eq!(segments[0].1, None);
        assert_eq!(segments[1].1, Some(TokenState::Resolved));
        assert_eq!(segments[2].1, None);
        assert_eq!(segments[3].1, Some(TokenState::Unresolved));
    }

    #[test]
    fn secret_display_counts_characters_not_bytes() {
        let segments = secret_display("é", None);
        assert_eq!(segments, [("•".to_string(), None)]);
        assert!(secret_display("", Some(&ctx())).is_empty());
    }

    #[test]
    fn labels_every_reference_raw_in_secret_mode() {
        let labels: Vec<_> = token_ranges("{{host}}{{nope}}{{!HOME}}", Some(&ctx()), true)
            .into_iter()
            .map(|(_, token)| token.label().to_string())
            .collect();
        assert_eq!(labels, ["{{host}}", "{{nope}}", "{{!HOME}}"]);
        let content = token_content("x{{host}}", Some(&ctx()), true);
        assert_eq!(content.tokens()[0].token().label().as_ref(), "{{host}}");
    }

    #[test]
    fn a_secret_field_hints_without_token_values() {
        let ctx = ctx();
        assert_eq!(
            reference_hint("{{host}}", Some(&ctx), false, 0.),
            "host = api.test"
        );
        assert_eq!(
            reference_hint("{{host}}", Some(&ctx), true, 0.),
            "host is defined"
        );
        assert_eq!(
            reference_hint("{{nope}}", Some(&ctx), true, 0.),
            "nope is not defined"
        );
        assert_eq!(
            reference_hint("{{!HOME}}", Some(&ctx), true, 0.),
            "{{!HOME}} reads the environment when sending"
        );
    }

    #[test]
    fn a_secret_field_suggests_names_without_values() {
        let options = token_options(Some(&ctx()));
        let host = options.iter().find(|option| option.name == "host").unwrap();
        assert_eq!(suggestion_value(host, false).as_deref(), Some("api.test"));
        assert_eq!(suggestion_value(host, true), None);
    }

    #[test]
    fn keeps_labels_free_of_control_characters() {
        assert_eq!(clean_label("a\nb", "{{x}}"), "a b");
        assert_eq!(clean_label("\n", "{{x}}"), "{{x}}");
    }
}
