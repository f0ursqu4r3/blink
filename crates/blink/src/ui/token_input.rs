//! A single-line text field that shows `{{token}}` references resolved and
//! suggests token names. Port of `TokenInput.vue`.
//!
//! The owner sets the value and the token context; user edits arrive as
//! `TokenInputEvent::Change` and are never echoed back by `set_value`.

use blink_core::interpolation::InterpolationContext;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;

#[derive(Debug, Clone)]
pub enum TokenInputEvent {
    /// The user changed the raw text.
    Change(String),
    PressEnter,
    Blur,
}

pub struct TokenInput {
    input: Entity<InputState>,
    context: Option<InterpolationContext>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TokenInputEvent> for TokenInput {}

impl TokenInput {
    pub fn new(placeholder: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = placeholder.into();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        TokenInput {
            input,
            context: None,
            _subscriptions: Vec::new(),
        }
    }

    /// Replace the text without emitting `Change`.
    pub fn set_value(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input
            .update(cx, |input, cx| input.set_value(value.to_string(), window, cx));
    }

    pub fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    /// Tokens for resolving and suggesting `{{name}}` references.
    pub fn set_context(&mut self, context: Option<InterpolationContext>, cx: &mut Context<Self>) {
        self.context = context;
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.read(cx).focus_handle(cx).focus(window, cx);
    }
}

impl Render for TokenInput {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        Input::new(&self.input)
    }
}
