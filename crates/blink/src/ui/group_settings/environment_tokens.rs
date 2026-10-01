//! The token table of a root group: a base value and a column per
//! environment. Port of `EnvironmentTokensEditor.vue`, plus the table
//! parsing of `GroupSettingsDialog.vue`.

use std::collections::{HashMap, HashSet};

use blink_core::definitions::rows_to_definitions;
use blink_core::environments::{
    ENVIRONMENT_COLORS, MAX_ENVIRONMENT_NAME, MAX_ENVIRONMENTS, create_environment,
    environment_color_label, next_environment_color,
};
use blink_core::model::{Definitions, Environment, EnvironmentColor};
use blink_core::request::pair;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::theme;
use crate::ui::widgets::{WIDEST, tracked};

/// One token as typed: a base value and a value per environment id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TokenRow {
    pub key: String,
    pub base: String,
    pub values: HashMap<u64, String>,
}

/// One environment column as typed.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: u64,
    pub name: String,
    pub color: EnvironmentColor,
    pub protected: bool,
}

/// Rows for the base tokens and every environment's tokens, with one blank
/// row when there are none.
pub fn to_rows(base: &Definitions, environments: &[Environment]) -> Vec<TokenRow> {
    let mut keys: Vec<&String> = base.keys().collect();
    for environment in environments {
        for key in environment.values.keys() {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    let rows: Vec<TokenRow> = keys
        .into_iter()
        .map(|key| TokenRow {
            key: key.clone(),
            base: base.get(key).cloned().unwrap_or_default(),
            values: environments
                .iter()
                .map(|e| (e.id, e.values.get(key).cloned().unwrap_or_default()))
                .collect(),
        })
        .collect();
    if rows.is_empty() {
        vec![TokenRow::default()]
    } else {
        rows
    }
}

/// Base tokens and environments from the table, or the error to show.
pub fn parse_table(
    rows: &[TokenRow],
    columns: &[Column],
) -> Result<(Definitions, Vec<Environment>), String> {
    let mut names = HashSet::new();
    for column in columns {
        let name = column.name.trim().to_uppercase();
        if name.is_empty() {
            return Err("Enter a name for each environment.".into());
        }
        if !names.insert(name.clone()) {
            return Err(format!("Environment \"{name}\" is defined more than once."));
        }
    }
    let used: Vec<&TokenRow> = rows
        .iter()
        .filter(|row| {
            !row.key.trim().is_empty()
                || !row.base.is_empty()
                || row.values.values().any(|value| !value.is_empty())
        })
        .collect();
    let check: Vec<_> = used
        .iter()
        .map(|row| {
            let base = if row.base.is_empty() { " " } else { row.base.as_str() };
            pair(row.key.clone(), base)
        })
        .collect();
    rows_to_definitions(&check)?;
    let value_in = |row: &TokenRow, id: u64| row.values.get(&id).filter(|v| !v.is_empty()).cloned();
    let mut base = Definitions::new();
    for row in &used {
        // A token set only in environments has no base value.
        let in_environment = columns.iter().any(|c| value_in(row, c.id).is_some());
        if !row.base.is_empty() || !in_environment {
            base.insert(row.key.trim().to_string(), row.base.clone());
        }
    }
    let environments = columns
        .iter()
        .map(|column| Environment {
            id: column.id,
            name: column.name.trim().to_uppercase(),
            color: column.color,
            protected: column.protected.then_some(true),
            values: used
                .iter()
                .filter_map(|row| value_in(row, column.id).map(|v| (row.key.trim().to_string(), v)))
                .collect(),
        })
        .collect();
    Ok((base, environments))
}

pub enum EnvironmentTokensEvent {
    Change,
}

struct RowInputs {
    id: u64,
    key: Entity<InputState>,
    base: Entity<InputState>,
    values: HashMap<u64, Entity<InputState>>,
    _subscriptions: Vec<Subscription>,
}

struct ColumnState {
    id: u64,
    name: Entity<InputState>,
    color: EnvironmentColor,
    protected: bool,
    menu_open: bool,
    _subscription: Subscription,
}

pub struct EnvironmentTokens {
    rows: Vec<RowInputs>,
    columns: Vec<ColumnState>,
}

impl EventEmitter<EnvironmentTokensEvent> for EnvironmentTokens {}

fn cell_input(
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<EnvironmentTokens>,
) -> Entity<InputState> {
    let value = value.to_string();
    let placeholder = placeholder.to_string();
    cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(value)
    })
}

impl EnvironmentTokens {
    pub fn new(
        rows: Vec<TokenRow>,
        environments: &[Environment],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = EnvironmentTokens {
            rows: Vec::new(),
            columns: Vec::new(),
        };
        for environment in environments {
            this.push_column(
                environment.id,
                &environment.name,
                environment.color,
                environment.protected == Some(true),
                window,
                cx,
            );
        }
        for row in rows {
            this.push_row(&row, window, cx);
        }
        this
    }

    /// Replace the rows, as when the group becomes a root group.
    pub fn set_rows(&mut self, rows: Vec<TokenRow>, window: &mut Window, cx: &mut Context<Self>) {
        self.rows.clear();
        for row in rows {
            self.push_row(&row, window, cx);
        }
        cx.notify();
    }

    pub fn rows(&self, cx: &App) -> Vec<TokenRow> {
        self.rows
            .iter()
            .map(|row| TokenRow {
                key: row.key.read(cx).value().to_string(),
                base: row.base.read(cx).value().to_string(),
                values: row
                    .values
                    .iter()
                    .map(|(id, input)| (*id, input.read(cx).value().to_string()))
                    .collect(),
            })
            .collect()
    }

    pub fn columns(&self, cx: &App) -> Vec<Column> {
        self.columns
            .iter()
            .map(|column| Column {
                id: column.id,
                name: column.name.read(cx).value().to_string(),
                color: column.color,
                protected: column.protected,
            })
            .collect()
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(EnvironmentTokensEvent::Change);
        cx.notify();
    }

    fn push_row(&mut self, row: &TokenRow, window: &mut Window, cx: &mut Context<Self>) {
        let id = pair("", "").id;
        let key = cell_input(&row.key, "Name", window, cx);
        let base = cell_input(&row.base, "Value", window, cx);
        let values: HashMap<u64, Entity<InputState>> = self
            .columns
            .iter()
            .map(|column| {
                let value = row.values.get(&column.id).cloned().unwrap_or_default();
                (column.id, cell_input(&value, &row.base, window, cx))
            })
            .collect();
        let mut subscriptions = vec![
            cx.subscribe(&key, |this, _, event: &InputEvent, cx| {
                if let InputEvent::Change = event {
                    this.changed(cx);
                }
            }),
            // An empty environment cell shows the base value it falls back to.
            cx.subscribe_in(&base, window, move |this, base, event, window, cx| {
                if let InputEvent::Change = event {
                    let placeholder = base.read(cx).value();
                    if let Some(row) = this.rows.iter().find(|row| row.id == id) {
                        for input in row.values.values() {
                            input.update(cx, |input, cx| {
                                input.set_placeholder(placeholder.clone(), window, cx)
                            });
                        }
                    }
                    this.changed(cx);
                }
            }),
        ];
        for input in values.values() {
            subscriptions.push(self.watch(input, cx));
        }
        self.rows.push(RowInputs {
            id,
            key,
            base,
            values,
            _subscriptions: subscriptions,
        });
    }

    fn watch(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe(input, |this, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                this.changed(cx);
            }
        })
    }

    fn push_column(
        &mut self,
        id: u64,
        name: &str,
        color: EnvironmentColor,
        protected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = name.to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Name")
                .default_value(name)
        });
        let subscription = cx.subscribe_in(&input, window, |this, state, event, window, cx| {
            if let InputEvent::Change = event {
                // As `maxlength`: keep at most MAX_ENVIRONMENT_NAME characters.
                let value = state.read(cx).value();
                if value.chars().count() > MAX_ENVIRONMENT_NAME {
                    let cut: String = value.chars().take(MAX_ENVIRONMENT_NAME).collect();
                    state.update(cx, |state, cx| state.set_value(cut, window, cx));
                }
                this.changed(cx);
            }
        });
        self.columns.push(ColumnState {
            id,
            name: input,
            color,
            protected,
            menu_open: false,
            _subscription: subscription,
        });
    }

    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.push_row(&TokenRow::default(), window, cx);
        self.changed(cx);
    }

    fn remove_row(&mut self, id: u64, cx: &mut Context<Self>) {
        self.rows.retain(|row| row.id != id);
        self.changed(cx);
    }

    fn add_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.columns.len() >= MAX_ENVIRONMENTS {
            return;
        }
        let existing: Vec<Environment> = self
            .columns(cx)
            .into_iter()
            .map(|column| create_environment(column.name, column.color))
            .collect();
        let name = if self.columns.is_empty() {
            "DEV".to_string()
        } else {
            format!("ENV {}", self.columns.len() + 1)
        };
        let environment = create_environment(name, next_environment_color(&existing));
        self.push_column(
            environment.id,
            &environment.name,
            environment.color,
            false,
            window,
            cx,
        );
        let mut subscriptions = Vec::new();
        for index in 0..self.rows.len() {
            let placeholder = self.rows[index].base.read(cx).value().to_string();
            let input = cell_input("", &placeholder, window, cx);
            subscriptions.push(self.watch(&input, cx));
            self.rows[index].values.insert(environment.id, input);
        }
        for (row, subscription) in self.rows.iter_mut().zip(subscriptions) {
            row._subscriptions.push(subscription);
        }
        self.changed(cx);
    }

    fn remove_column(&mut self, id: u64, cx: &mut Context<Self>) {
        self.columns.retain(|column| column.id != id);
        for row in &mut self.rows {
            row.values.remove(&id);
        }
        self.changed(cx);
    }

    fn column_mut(&mut self, id: u64) -> Option<&mut ColumnState> {
        self.columns.iter_mut().find(|column| column.id == id)
    }

    fn render_menu(&self, column: &ColumnState, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let id = column.id;
        let separator = || div().h(px(1.)).mx(px(-4.)).my(px(4.)).bg(colors.border);
        let item = |key: SharedString| {
            h_flex()
                .id(key)
                .gap(px(8.))
                .h(px(28.))
                .px(px(8.))
                .rounded(px(4.))
                .text_size(px(13.))
                .cursor_pointer()
                .hover(move |style| style.bg(colors.accent))
        };
        v_flex()
            .w(px(208.))
            .p(px(4.))
            .child(
                div().px(px(8.)).py(px(6.)).font_family(theme::MONO).child(
                    Input::new(&column.name)
                        .small()
                        .h(px(28.))
                        .aria_label("Environment name"),
                ),
            )
            .child(separator())
            .child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.muted_foreground)
                    .child("Color"),
            )
            .children(ENVIRONMENT_COLORS.iter().map(|color| {
                let color = *color;
                let selected = column.color == color;
                item(SharedString::from(format!("env-{id}-color-{color:?}")))
                    .child(
                        div()
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(selected, |this| {
                                this.child(div().size(px(6.)).rounded_full().bg(colors.foreground))
                            }),
                    )
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(theme::environment_color(color, cx)),
                    )
                    .child(environment_color_label(color))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(column) = this.column_mut(id) {
                            column.color = color;
                        }
                        this.changed(cx);
                    }))
            }))
            .child(separator())
            .child(
                item(SharedString::from(format!("env-{id}-protected")))
                    .child(
                        Checkbox::new(SharedString::from(format!("env-{id}-protected-check")))
                            .checked(column.protected)
                            .xsmall()
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                if let Some(column) = this.column_mut(id) {
                                    column.protected = *checked;
                                }
                                this.changed(cx);
                            })),
                    )
                    .child("Protected")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(column) = this.column_mut(id) {
                            column.protected = !column.protected;
                        }
                        this.changed(cx);
                    })),
            )
            .child(separator())
            .child(
                item(SharedString::from(format!("env-{id}-delete")))
                    .text_color(colors.destructive)
                    .child(Icon::new(IconName::Trash).size(px(13.)))
                    .child("Delete environment")
                    .on_click(cx.listener(move |this, _, _, cx| this.remove_column(id, cx))),
            )
            .into_any_element()
    }
}

const NAME_WIDTH: f32 = 144.;
const VALUE_MIN_WIDTH: f32 = 160.;
const ENV_WIDTH: f32 = 144.;
const SIDE_WIDTH: f32 = 32.;

impl Render for EnvironmentTokens {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let head = |text: &'static str| {
            div()
                .h_full()
                .flex()
                .items_center()
                .px(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .child(tracked(text.to_uppercase(), WIDEST))
        };
        let mut header = h_flex()
            .h(px(32.))
            .bg(colors.muted)
            .text_size(px(10.))
            .text_color(colors.muted_foreground)
            .child(head("Name").w(px(NAME_WIDTH)).flex_none())
            .child(
                head("Value")
                    .flex_1()
                    .min_w(px(VALUE_MIN_WIDTH))
                    .border_l_1()
                    .border_color(colors.border),
            );
        for index in 0..self.columns.len() {
            let column = &self.columns[index];
            let id = column.id;
            let name = column.name.read(cx).value().to_uppercase();
            let label = if name.is_empty() { "UNNAMED".to_string() } else { name };
            let menu = self.render_menu(column, cx);
            let color = theme::environment_color(column.color, cx);
            header = header.child(
                div()
                    .w(px(ENV_WIDTH))
                    .flex_none()
                    .h_full()
                    .border_l_1()
                    .border_color(colors.border)
                    .child(
                        Popover::new(SharedString::from(format!("env-menu-{id}")))
                            .open(column.menu_open)
                            .on_open_change(cx.listener(move |this, open: &bool, _, cx| {
                                if let Some(column) = this.column_mut(id) {
                                    column.menu_open = *open;
                                }
                                cx.notify();
                            }))
                            .trigger_style({
                                let mut style = StyleRefinement::default();
                                style.size.width = Some(relative(1.).into());
                                style.size.height = Some(relative(1.).into());
                                style
                            })
                            .trigger(
                                Button::new(SharedString::from(format!("env-header-{id}")))
                                    .ghost()
                                    .w_full()
                                    .h(px(32.))
                                    .rounded(px(0.))
                                    .accessibility_label(format!("{label} environment options"))
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .gap(px(6.))
                                            .font_family(theme::MONO)
                                            .text_size(px(10.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(color)
                                            .child(div().truncate().child(tracked(label.clone(), WIDEST)))
                                            .when(column.protected, |this| {
                                                this.child(
                                                    div()
                                                        .text_size(px(9.))
                                                        .font_weight(FontWeight::NORMAL)
                                                        .text_color(colors.muted_foreground)
                                                        .child("protected"),
                                                )
                                            }),
                                    ),
                            )
                            .p_0()
                            .child(menu),
                    ),
            );
        }
        header = header.child(
            div()
                .w(px(SIDE_WIDTH))
                .flex_none()
                .border_l_1()
                .border_color(colors.border)
                .child(
                    Button::new("add-environment")
                        .ghost()
                        .size(px(32.))
                        .rounded(px(0.))
                        .icon(Icon::new(IconName::Plus).size(px(13.)))
                        .tooltip("Add environment")
                        .disabled(self.columns.len() >= MAX_ENVIRONMENTS)
                        .on_click(cx.listener(|this, _, window, cx| this.add_column(window, cx))),
                ),
        );

        let cell = || {
            div()
                .h(px(34.))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(colors.border)
        };
        let rows = self.rows.iter().enumerate().map(|(index, row)| {
            let id = row.id;
            let mut line = h_flex()
                .id(SharedString::from(format!("token-row-{id}")))
                .font_family(theme::MONO)
                .child(
                    cell().w(px(NAME_WIDTH)).flex_none().child(
                        Input::new(&row.key)
                            .appearance(false)
                            .small()
                            .aria_label(format!("Token name {}", index + 1)),
                    ),
                )
                .child(
                    cell()
                        .flex_1()
                        .min_w(px(VALUE_MIN_WIDTH))
                        .border_l_1()
                        .child(
                            Input::new(&row.base)
                                .appearance(false)
                                .small()
                                .aria_label(format!("Token {} value", index + 1)),
                        ),
                );
            for column in &self.columns {
                if let Some(input) = row.values.get(&column.id) {
                    let empty = input.read(cx).value().is_empty();
                    let name = column.name.read(cx).value().to_string();
                    line = line.child(
                        cell()
                            .id(SharedString::from(format!("token-{id}-env-{}", column.id)))
                            .w(px(ENV_WIDTH))
                            .flex_none()
                            .border_l_1()
                            .child(
                                Input::new(input)
                                    .appearance(false)
                                    .small()
                                    .aria_label(format!("Token {} value in {name}", index + 1)),
                            )
                            .when(empty, |this| {
                                this.tooltip(|window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(
                                        "Empty uses the base value",
                                    )
                                    .build(window, cx)
                                })
                            }),
                    );
                }
            }
            line.child(
                cell().w(px(SIDE_WIDTH)).flex_none().border_l_1().child(
                    Button::new(SharedString::from(format!("remove-token-{id}")))
                        .ghost()
                        .size(px(32.))
                        .rounded(px(0.))
                        .icon(Icon::new(IconName::X).size(px(13.)))
                        .accessibility_label(format!("Remove token {}", index + 1))
                        .on_click(cx.listener(move |this, _, _, cx| this.remove_row(id, cx))),
                ),
            )
        });

        let width = px(NAME_WIDTH
            + VALUE_MIN_WIDTH
            + ENV_WIDTH * self.columns.len() as f32
            + SIDE_WIDTH);
        div()
            .id("environment-tokens")
            .overflow_x_scroll()
            .child(
                v_flex()
                    .min_w(width)
                    .w_full()
                    .child(header)
                    .children(rows)
                    .child(
                        div().p(px(4.)).child(
                            Button::new("add-token")
                                .ghost()
                                .small()
                                .icon(Icon::new(IconName::Plus).size(px(13.)))
                                .label("Add value")
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.add_row(window, cx)),
                                ),
                        ),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn defs(entries: &[(&str, &str)]) -> Definitions {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn column(id: u64, name: &str) -> Column {
        Column {
            id,
            name: name.into(),
            color: EnvironmentColor::Info,
            protected: false,
        }
    }

    fn row(key: &str, base: &str, values: &[(u64, &str)]) -> TokenRow {
        TokenRow {
            key: key.into(),
            base: base.into(),
            values: values.iter().map(|(id, v)| (*id, v.to_string())).collect(),
        }
    }

    #[test]
    fn gives_one_blank_row_for_no_tokens() {
        assert_eq!(to_rows(&Definitions::new(), &[]), vec![TokenRow::default()]);
    }

    #[test]
    fn merges_base_and_environment_keys() {
        let mut dev = create_environment("DEV", EnvironmentColor::Info);
        dev.values = defs(&[("host", "dev.test"), ("only", "x")]);
        let rows = to_rows(&defs(&[("host", "prod.test")]), std::slice::from_ref(&dev));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].base, "prod.test");
        assert_eq!(rows[0].values[&dev.id], "dev.test");
        assert_eq!(rows[1].key, "only");
        assert_eq!(rows[1].base, "");
    }

    #[test]
    fn saves_environment_values_and_skips_empty_cells() {
        let rows = vec![
            row("host", "prod.test", &[(7, "dev.test")]),
            row("token", "abc", &[(7, "")]),
            row("devonly", "", &[(7, "1")]),
            row("", "", &[]),
        ];
        let (base, environments) = parse_table(&rows, &[column(7, " dev ")]).unwrap();
        assert_eq!(base, defs(&[("host", "prod.test"), ("token", "abc")]));
        assert_eq!(environments[0].name, "DEV");
        assert_eq!(
            environments[0].values,
            defs(&[("host", "dev.test"), ("devonly", "1")])
        );
        assert_eq!(environments[0].protected, None);
    }

    #[test]
    fn rejects_blank_and_duplicate_environment_names() {
        assert_eq!(
            parse_table(&[], &[column(1, " ")]),
            Err("Enter a name for each environment.".into())
        );
        assert_eq!(
            parse_table(&[], &[column(1, "dev"), column(2, "DEV")]),
            Err("Environment \"DEV\" is defined more than once.".into())
        );
    }

    #[test]
    fn rejects_a_value_without_a_name() {
        assert_eq!(
            parse_table(&[row("", "x", &[])], &[]),
            Err("Enter a name for each token.".into())
        );
    }
}
