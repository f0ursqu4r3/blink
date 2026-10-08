//! Theme selection applies immediately. Hover previews never reach storage.

use std::collections::HashMap;
use std::rc::Rc;

use blink_core::engine::GhosttyTheme;
use blink_core::theme::{ACCENT_SLOTS, AccentSlot, DEFAULT_ACCENT, ThemeSetting, is_dark};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::select::{
    SearchableVec, Select, SelectDelegate, SelectEvent, SelectGroup, SelectItem, SelectState,
};
use gpui_kit::component::{Disableable as _, IndexPath, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::layout;
use crate::store::Store;
use crate::theme::{self, AppTheme};
use crate::ui::form::{Choice, choice_index, u};

const PLACEHOLDER: &str = "background = #1e1e1e\nforeground = #d4d4d4\npalette = 3=#dcdcaa";
const DEFAULT_LABEL: &str = "Blink (default)";

pub struct ThemeSettings {
    store: Entity<Store>,
    themes: Vec<GhosttyTheme>,
    read_error: String,
    storage_error: String,
    edit_error: String,
    _activation: Subscription,
    theme_select: Entity<ThemeSelect>,
    selected: Option<ThemeSetting>,
    cache: HashMap<String, String>,
    hovered: Option<String>,
    selecting: bool,
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

/// Theme options in Dark and Light sections. The default leads Dark. A draft
/// that is not an installed theme (such as "Custom") leads its section.
fn theme_groups(
    themes: &[GhosttyTheme],
    draft: Option<&ThemeSetting>,
) -> Vec<(&'static str, Vec<Choice>)> {
    let mut dark = vec![Choice::new("", DEFAULT_LABEL)];
    let mut light = Vec::new();
    if let Some(draft) = draft
        && !themes.iter().any(|theme| theme.name == draft.name)
    {
        let group = if is_dark(&draft.text) {
            &mut dark
        } else {
            &mut light
        };
        group.push(Choice::same(draft.name.clone()));
    }
    for theme in themes {
        let group = if theme.dark { &mut dark } else { &mut light };
        group.push(Choice::same(theme.name.clone()));
    }
    [("Dark", dark), ("Light", light)]
        .into_iter()
        .filter(|(_, choices)| !choices.is_empty())
        .collect()
}

type ThemeSelect = SelectState<ThemeList>;

fn theme_list(
    themes: &[GhosttyTheme],
    draft: Option<&ThemeSetting>,
    owner: WeakEntity<ThemeSettings>,
) -> ThemeList {
    let mut items = SearchableVec::new(Vec::new());
    for (title, choices) in theme_groups(themes, draft) {
        items.push(
            SelectGroup::new(title).items(choices.into_iter().map(|choice| ThemeItem {
                choice,
                owner: owner.clone(),
            })),
        );
    }
    ThemeList(items)
}

/// A theme option and the settings it previews in.
#[derive(Clone)]
struct ThemeItem {
    choice: Choice,
    owner: WeakEntity<ThemeSettings>,
}

impl SelectItem for ThemeItem {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.choice.label.clone()
    }

    fn value(&self) -> &SharedString {
        &self.choice.value
    }

    fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let label = self.choice.label.clone();
        div()
            .debug_selector({
                let label = label.clone();
                move || format!("theme-choice-{label}")
            })
            .child(label)
    }
}

/// The grouped theme options. Hovering a whole row previews its theme.
struct ThemeList(SearchableVec<SelectGroup<ThemeItem>>);

impl SelectDelegate for ThemeList {
    type Item = ThemeItem;

    fn sections_count(&self, cx: &App) -> usize {
        self.0.sections_count(cx)
    }

    #[allow(deprecated)]
    fn section(&self, section: usize) -> Option<AnyElement> {
        self.0.section(section)
    }

    fn items_count(&self, section: usize) -> usize {
        self.0.items_count(section)
    }

    fn item(&self, ix: IndexPath) -> Option<&ThemeItem> {
        self.0.item(ix)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        ThemeItem: SelectItem<Value = V>,
        V: PartialEq,
    {
        self.0.position(value)
    }

    fn perform_search(&mut self, query: &str, window: &mut Window, cx: &mut App) -> Task<()> {
        self.0.perform_search(query, window, cx)
    }

    fn item_hover_listener(
        &self,
        _: IndexPath,
        item: &ThemeItem,
    ) -> Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>> {
        let name = item.choice.value.to_string();
        let owner = item.owner.clone();
        Some(Rc::new(move |hovered, window, cx| {
            owner
                .update(cx, |this, cx| {
                    if *hovered {
                        this.hover(name.clone(), window, cx);
                    } else if this.hovered.as_ref() == Some(&name) {
                        this.end_preview(cx);
                    }
                })
                .ok();
        }))
    }
}

impl ThemeSettings {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = draft(cx);
        // The default and the current draft are available before the
        // installed themes finish loading.
        let items = theme_list(&[], current.as_ref(), cx.entity().downgrade());
        let name = SharedString::from(current.as_ref().map(|d| d.name.clone()).unwrap_or_default());
        let theme_select = cx.new(|cx| {
            let index = items.position(&name);
            SelectState::new(items, index, window, cx).searchable(true)
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
                |this, _, event: &SelectEvent<ThemeList>, window, cx| {
                    let SelectEvent::Confirm(Some(name)) = event else {
                        return;
                    };
                    // The chosen theme replaces the preview; closing the menu
                    // must not restore the previous selection first.
                    this.hovered = None;
                    this.choose(name.to_string(), window, cx);
                },
            ),
            cx.subscribe(&theme_select, |this, _, _: &DismissEvent, cx| {
                this.end_preview(cx);
            }),
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
                    // Ignore a text notification that already matches the selection.
                    if text
                        != this
                            .selected
                            .as_ref()
                            .map(|s| s.text.as_str())
                            .unwrap_or("")
                        || !this.edit_error.is_empty()
                    {
                        this.edit(text, window, cx);
                    }
                }
            }),
        ];
        let list = store.read(cx).engine.list_ghostty_themes();
        cx.spawn_in(window, async move |this, cx| {
            let themes = list.await;
            this.update_in(cx, |this, window, cx| {
                this.themes = themes;
                this.sync_choices(window, cx);
            })
            .ok();
        })
        .detach();
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.end_preview(cx);
            }
        });
        ThemeSettings {
            store,
            themes: Vec::new(),
            read_error: String::new(),
            storage_error: String::new(),
            edit_error: String::new(),
            _activation: activation,
            theme_select,
            selected: current,
            cache: HashMap::new(),
            hovered: None,
            selecting: false,
            colors,
            accent_select,
            request: 0,
            _subscriptions: subscriptions,
        }
    }

    fn accent(&self) -> AccentSlot {
        self.selected.as_ref().map_or(DEFAULT_ACCENT, |d| d.accent)
    }

    fn preview(next: Option<ThemeSetting>, cx: &mut App) {
        theme::update_theme(cx, |theme| theme.state.preview(next));
    }

    fn sync_choices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = theme_list(
            &self.themes,
            self.selected.as_ref(),
            cx.entity().downgrade(),
        );
        let name = SharedString::from(
            self.selected
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
        );
        self.theme_select.update(cx, |select, cx| {
            select.set_items(items, window, cx);
            select.set_selected_value(&name, window, cx);
        });
        let accent = SharedString::from(self.accent().to_string());
        self.accent_select.update(cx, |select, cx| {
            select.set_selected_value(&accent, window, cx);
        });
        cx.notify();
    }

    fn sync_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self
            .selected
            .as_ref()
            .map(|d| d.text.clone())
            .unwrap_or_default();
        self.colors
            .update(cx, |colors, cx| colors.set_value(text, window, cx));
    }

    fn apply(&mut self, next: Option<ThemeSetting>, cx: &mut Context<Self>) {
        Self::preview(next.clone(), cx);
        if cx.global::<AppTheme>().state.error.is_empty() {
            self.selected = next;
            self.storage_error = theme::update_theme(cx, |theme| {
                theme.commit();
                theme.state.save_error.clone()
            });
        }
        cx.notify();
    }

    pub(super) fn end_preview(&mut self, cx: &mut Context<Self>) {
        if self.hovered.take().is_none() {
            return;
        }
        if !self.selecting {
            self.request += 1;
        }
        Self::preview(self.selected.clone(), cx);
        cx.notify();
    }

    pub(super) fn cancel_preview(&mut self, cx: &mut Context<Self>) {
        self.request += 1;
        self.selecting = false;
        self.end_preview(cx);
    }

    pub(super) fn choose(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.load(name, true, window, cx);
    }

    pub(super) fn hover(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.selecting {
            return;
        }
        self.hovered = Some(name.clone());
        self.load(name, false, window, cx);
    }

    fn show(
        &mut self,
        name: String,
        text: Option<String>,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = text.map(|text| ThemeSetting {
            name,
            text,
            accent: self.accent(),
        });
        if select {
            self.edit_error.clear();
            self.selecting = false;
            self.apply(next, cx);
            self.sync_text(window, cx);
            self.sync_choices(window, cx);
        } else {
            Self::preview(next, cx);
        }
    }

    fn load(&mut self, name: String, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.read_error.clear();
        self.request += 1;
        self.selecting = select;
        let current = self.request;
        if name.is_empty() {
            self.show(name, None, select, window, cx);
            return;
        }
        let text = self
            .selected
            .as_ref()
            .filter(|s| s.name == name)
            .map(|s| s.text.clone())
            .or_else(|| self.cache.get(&name).cloned());
        if let Some(text) = text {
            self.show(name, Some(text), select, window, cx);
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
                        this.cache.insert(name.clone(), text.clone());
                        this.show(name, Some(text), select, window, cx);
                    }
                    Err(reason) => {
                        this.selecting = false;
                        this.read_error = reason;
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn edit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        self.selecting = false;
        self.hovered = None;
        if text.trim().is_empty() {
            self.apply(None, cx);
        } else {
            let accent = self.accent();
            self.apply(
                Some(ThemeSetting {
                    name: "Custom".into(),
                    text,
                    accent,
                }),
                cx,
            );
        }
        self.edit_error = cx.global::<AppTheme>().state.error.clone();
        self.sync_choices(window, cx);
    }

    fn set_accent(&mut self, accent: AccentSlot, window: &mut Window, cx: &mut Context<Self>) {
        self.request += 1;
        self.selecting = false;
        self.hovered = None;
        if let Some(current) = self.selected.clone() {
            self.apply(Some(ThemeSetting { accent, ..current }), cx);
        }
        self.sync_choices(window, cx);
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_error.clear();
        self.request += 1;
        self.selecting = false;
        self.hovered = None;
        self.apply(None, cx);
        self.sync_text(window, cx);
        self.sync_choices(window, cx);
    }
}

impl Render for ThemeSettings {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let state = &cx.global::<AppTheme>().state;
        let has_draft = self.selected.is_some();
        let message = if !self.edit_error.is_empty() {
            self.edit_error.clone()
        } else if state.error.is_empty() {
            self.storage_error.clone()
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
        let theme_note = if self.read_error.is_empty() {
            crate::ui::form::note(
                "Hover a theme in the menu to preview it.",
                colors.muted_foreground,
            )
        } else {
            crate::ui::form::note(self.read_error.clone(), colors.destructive)
        };
        let custom = v_flex()
            .gap(u(6.))
            .child(
                Textarea::new(&self.colors)
                    .h(u(112.))
                    .font_family(theme::MONO)
                    .text_size(u(12.)),
            )
            .when(!message.is_empty(), |this| this.child(error_text(message)));
        let heading = h_flex()
            .justify_between()
            .child(crate::ui::form::section_heading("Theme", cx))
            .child(
                Button::new("theme-reset")
                    .ghost()
                    .label("Reset to default")
                    // XSmall sets the label to `text-xs`.
                    .xsmall()
                    .font_family(theme::MONO)
                    .disabled(!has_draft)
                    .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
            );
        layout::group(
            heading,
            vec![
                layout::row(
                    "Ghostty theme",
                    Some(theme_note),
                    layout::field(
                        div().debug_selector(|| "theme-select".into()).child(
                            Select::new(&self.theme_select)
                                .font_family(theme::MONO)
                                .text_size(u(12.))
                                .search_placeholder("Search themes")
                                .menu_max_h(u(320.)),
                        ),
                    ),
                    cx,
                ),
                layout::stacked(
                    "Custom colors",
                    Some(crate::ui::form::note(
                        "The selected theme as Ghostty config. Edits save as a custom theme.",
                        colors.muted_foreground,
                    )),
                    custom,
                    cx,
                ),
                layout::row(
                    "Accent",
                    Some(crate::ui::form::note(
                        "The palette slot that colors active and focused controls.",
                        colors.muted_foreground,
                    )),
                    layout::field(
                        Select::new(&self.accent_select)
                            .font_family(theme::MONO)
                            .text_size(u(12.))
                            .disabled(!has_draft),
                    ),
                    cx,
                ),
            ],
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[gpui_kit::test]
    fn undo_to_selected_text_clears_the_parse_error(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let engine = crate::test_support::engine(dir.path());
        crate::test_support::init(cx, &engine);
        let harness = crate::test_support::open(cx, &engine);
        let editor = harness.update(cx, |window, cx| {
            cx.new(|cx| ThemeSettings::new(harness.store.clone(), window, cx))
        });
        let input = cx.read(|cx| editor.read(cx).colors.clone());
        for text in [
            "background = #ffffff",
            "background = invalid",
            "background = #ffffff",
        ] {
            harness.update(cx, |window, cx| {
                input.update(cx, |input, cx| {
                    input.set_value(text, window, cx);
                    cx.emit(InputEvent::Change);
                });
            });
            cx.run_until_parked();
        }
        assert!(cx.read(|cx| editor.read(cx).edit_error.is_empty()));
        assert!(cx.read(|cx| cx.global::<AppTheme>().state.error.is_empty()));
    }

    fn setting(name: &str) -> ThemeSetting {
        ThemeSetting {
            name: name.into(),
            text: "background = #000000".into(),
            accent: 4,
        }
    }

    fn installed(name: &str, dark: bool) -> GhosttyTheme {
        GhosttyTheme {
            name: name.into(),
            dark,
        }
    }

    fn values(groups: Vec<(&'static str, Vec<Choice>)>) -> Vec<(&'static str, Vec<String>)> {
        groups
            .into_iter()
            .map(|(title, choices)| {
                let values = choices.into_iter().map(|c| c.value.to_string()).collect();
                (title, values)
            })
            .collect()
    }

    #[test]
    fn sorts_the_default_and_installed_themes_into_dark_and_light() {
        let themes = [installed("Dracula", true), installed("Paper", false)];
        assert_eq!(
            values(theme_groups(&themes, None)),
            [
                ("Dark", vec!["".to_string(), "Dracula".to_string()]),
                ("Light", vec!["Paper".to_string()]),
            ]
        );
        assert_eq!(
            values(theme_groups(&[installed("Nord", true)], None)),
            [("Dark", vec!["".to_string(), "Nord".to_string()])]
        );
    }

    #[test]
    fn adds_a_custom_draft_that_is_not_installed() {
        let themes = [installed("Nord", true)];
        let custom = ThemeSetting {
            text: "background = #ffffff\nforeground = #000000".into(),
            ..setting("Custom")
        };
        assert_eq!(
            values(theme_groups(&themes, Some(&custom))),
            [
                ("Dark", vec!["".to_string(), "Nord".to_string()]),
                ("Light", vec!["Custom".to_string()]),
            ]
        );
        let nord = setting("Nord");
        assert_eq!(theme_groups(&themes, Some(&nord))[0].1.len(), 2);
    }

    #[test]
    fn labels_accent_slots_with_their_number() {
        assert_eq!(accent_choices()[3].label.as_ref(), "Blue (4)");
    }
}
