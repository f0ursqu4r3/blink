//! The theme section of Application Settings. Port of `ThemeSettings.vue`.
//!
//! Edits preview live through the global `AppTheme`; the dialog commits or
//! reverts them.

use blink_core::theme::{ACCENT_SLOTS, AccentSlot, DEFAULT_ACCENT, ThemeSetting};
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme::{self, AppTheme};
use crate::ui::form::{Choice, choice_index, field_label, u};

const PLACEHOLDER: &str = "background = #1e1e1e\nforeground = #d4d4d4\npalette = 3=#dcdcaa";
const DEFAULT_LABEL: &str = "Blink (default)";

pub struct ThemeSettings {
    store: Entity<Store>,
    names: Vec<String>,
    read_error: String,
    theme_select: Entity<SelectState<Vec<Choice>>>,
    colors: Entity<TextareaState>,
    accent_select: Entity<SelectState<Vec<Choice>>>,
    /// Guards against a slow theme read overwriting a newer edit, reset,
    /// theme choice, or accent change made while the read was in flight.
    request: u64,
    _subscriptions: Vec<Subscription>,
}

fn draft(cx: &App) -> Option<ThemeSetting> {
    cx.global::<AppTheme>().state.draft.clone()
}

fn accent_choices() -> Vec<Choice> {
    ACCENT_SLOTS
        .iter()
        .map(|(slot, label)| Choice::new(slot.to_string(), format!("{label} ({slot})")))
        .collect()
}

/// Theme options: the default, the draft when it is not an installed
/// theme (such as "Custom"), then the installed themes.
fn theme_choices(names: &[String], draft: Option<&ThemeSetting>) -> Vec<Choice> {
    let mut choices = vec![Choice::new("", DEFAULT_LABEL)];
    if let Some(draft) = draft
        && !names.contains(&draft.name)
    {
        choices.push(Choice::same(draft.name.clone()));
    }
    choices.extend(names.iter().map(|name| Choice::same(name.clone())));
    choices
}

impl ThemeSettings {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = draft(cx);
        let theme_select = cx.new(|cx| {
            let choices = theme_choices(&[], current.as_ref());
            let index = choice_index(&choices, current.as_ref().map_or("", |d| d.name.as_str()));
            SelectState::new(choices, index, window, cx).searchable(true)
        });
        let colors = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(6)
                .placeholder(PLACEHOLDER)
                .default_value(current.as_ref().map(|d| d.text.clone()).unwrap_or_default())
        });
        let accent = current.as_ref().map_or(DEFAULT_ACCENT, |d| d.accent);
        let accent_select = cx.new(|cx| {
            let choices = accent_choices();
            let index = choice_index(&choices, &accent.to_string());
            SelectState::new(choices, index, window, cx)
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &theme_select,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, window, cx| {
                    let SelectEvent::Confirm(value) = event;
                    let name = value.as_ref().map(|v| v.to_string()).unwrap_or_default();
                    this.choose(name, window, cx);
                },
            ),
            cx.subscribe_in(
                &accent_select,
                window,
                |this, _, event: &SelectEvent<Vec<Choice>>, window, cx| {
                    let SelectEvent::Confirm(Some(value)) = event else {
                        return;
                    };
                    if let Ok(slot) = value.parse::<AccentSlot>() {
                        this.set_accent(slot, window, cx);
                    }
                },
            ),
            cx.subscribe_in(&colors, window, |this, state, event, window, cx| {
                if let InputEvent::Change = event {
                    let text = state.read(cx).value().to_string();
                    this.edit(text, window, cx);
                }
            }),
        ];
        let list = store.read(cx).engine.list_ghostty_themes();
        cx.spawn_in(window, async move |this, cx| {
            let names = list.await;
            this.update_in(cx, |this, window, cx| {
                this.names = names;
                this.sync_choices(window, cx);
            })
            .ok();
        })
        .detach();
        ThemeSettings {
            store,
            names: Vec::new(),
            read_error: String::new(),
            theme_select,
            colors,
            accent_select,
            request: 0,
            _subscriptions: subscriptions,
        }
    }

    fn accent(cx: &App) -> AccentSlot {
        draft(cx).map_or(DEFAULT_ACCENT, |d| d.accent)
    }

    fn preview(next: Option<ThemeSetting>, cx: &mut App) {
        theme::update_theme(cx, |theme| theme.state.preview(next));
    }

    /// Show the draft in the theme select, the text, and the accent.
    fn sync_choices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = draft(cx);
        let choices = theme_choices(&self.names, current.as_ref());
        let name = current.as_ref().map_or(String::new(), |d| d.name.clone());
        self.theme_select.update(cx, |select, cx| {
            select.set_items(choices, window, cx);
            select.set_selected_value(&SharedString::from(name), window, cx);
        });
        let accent = SharedString::from(Self::accent(cx).to_string());
        self.accent_select.update(cx, |select, cx| {
            select.set_selected_value(&accent, window, cx);
        });
        cx.notify();
    }

    fn sync_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = draft(cx).map(|d| d.text).unwrap_or_default();
        self.colors
            .update(cx, |colors, cx| colors.set_value(text, window, cx));
    }

    fn choose(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.read_error.clear();
        self.request += 1;
        let current = self.request;
        if name.is_empty() {
            Self::preview(None, cx);
            self.sync_text(window, cx);
            self.sync_choices(window, cx);
            return;
        }
        let read = self.store.read(cx).engine.read_ghostty_theme(name.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            this.update_in(cx, |this, window, cx| {
                if current != this.request {
                    return;
                }
                match result {
                    Ok(text) => {
                        let accent = Self::accent(cx);
                        Self::preview(Some(ThemeSetting { name, text, accent }), cx);
                        this.sync_text(window, cx);
                    }
                    Err(reason) => this.read_error = reason,
                }
                this.sync_choices(window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn edit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        if text.trim().is_empty() {
            Self::preview(None, cx);
        } else {
            let accent = Self::accent(cx);
            Self::preview(
                Some(ThemeSetting {
                    name: "Custom".into(),
                    text,
                    accent,
                }),
                cx,
            );
        }
        self.sync_choices(window, cx);
    }

    fn set_accent(&mut self, accent: AccentSlot, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        if let Some(current) = draft(cx) {
            Self::preview(Some(ThemeSetting { accent, ..current }), cx);
        }
        self.sync_choices(window, cx);
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        Self::preview(None, cx);
        self.sync_text(window, cx);
        self.sync_choices(window, cx);
    }
}

impl Render for ThemeSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let state = &cx.global::<AppTheme>().state;
        let has_draft = state.draft.is_some();
        let message = if state.error.is_empty() {
            state.save_error.clone()
        } else {
            state.error.clone()
        };
        let error_text = |text: String| {
            div()
                .font_family(theme::MONO)
                .text_size(u(12.))
                .text_color(colors.destructive)
                .child(text)
        };
        v_flex()
            .gap(u(8.))
            .border_t_1()
            .border_color(colors.border)
            .pt(u(12.))
            .text_size(u(12.))
            .child(crate::ui::form::section_heading("Theme", cx))
            .child(
                v_flex()
                    .gap(u(6.))
                    .child(field_label("Ghostty theme", cx))
                    .child(
                        Select::new(&self.theme_select)
                            .font_family(theme::MONO)
                            .text_size(u(12.))
                            .search_placeholder("Search themes"),
                    )
                    .when(!self.read_error.is_empty(), |this| {
                        this.child(error_text(self.read_error.clone()))
                    }),
            )
            .child(
                v_flex()
                    .gap(u(6.))
                    .child(field_label("Ghostty colors", cx))
                    .child(
                        Textarea::new(&self.colors)
                            .h(u(112.))
                            .font_family(theme::MONO)
                            .text_size(u(12.)),
                    )
                    .when(!message.is_empty(), |this| this.child(error_text(message))),
            )
            .child(
                h_flex()
                    .items_end()
                    .gap(u(12.))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(u(6.))
                            .child(field_label("Accent", cx))
                            .child(
                                Select::new(&self.accent_select)
                                    .font_family(theme::MONO)
                                    .text_size(u(12.))
                                    .disabled(!has_draft),
                            ),
                    )
                    .child(
                        Button::new("theme-reset")
                            .outline()
                            .label("Reset to default")
                            // XSmall sets the label to `text-xs`.
                            .xsmall()
                            .h(u(32.))
                            .px(u(12.))
                            .font_family(theme::MONO)
                            .disabled(!has_draft)
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn setting(name: &str) -> ThemeSetting {
        ThemeSetting {
            name: name.into(),
            text: "background = #000000".into(),
            accent: 4,
        }
    }

    #[test]
    fn lists_the_default_then_installed_themes() {
        let names = vec!["Nord".to_string(), "Dracula".to_string()];
        let values: Vec<_> = theme_choices(&names, None)
            .into_iter()
            .map(|c| c.label.to_string())
            .collect();
        assert_eq!(values, ["Blink (default)", "Nord", "Dracula"]);
    }

    #[test]
    fn adds_a_custom_draft_that_is_not_installed() {
        let names = vec!["Nord".to_string()];
        let custom = setting("Custom");
        let values: Vec<_> = theme_choices(&names, Some(&custom))
            .into_iter()
            .map(|c| c.value.to_string())
            .collect();
        assert_eq!(values, ["", "Custom", "Nord"]);
        let nord = setting("Nord");
        assert_eq!(theme_choices(&names, Some(&nord)).len(), 2);
    }

    #[test]
    fn labels_accent_slots_with_their_number() {
        assert_eq!(accent_choices()[3].label.as_ref(), "Blue (4)");
    }
}
