//! Language features of the body editors, as `CodeEditor.vue` and
//! `src/lib/code-editor.ts` set them up: syntax colors from the app theme,
//! GraphQL highlighting and schema completions, and `{{token}}` coloring,
//! hints, and completions.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Once};

use anyhow::Result;
use blink_core::graphql_schema::{CompletionKind, GraphqlSchema, graphql_completions};
use blink_core::interpolation::InterpolationContext;
use blink_core::token_hints::{
    TokenScope, TokenSpan, TokenState, match_token_options, token_hint, token_options,
    token_query_at, token_ranges,
};
use gpui_kit::component::highlighter::{HighlightTheme, LanguageRegistry};
use gpui_kit::component::input::{
    CompletionProvider, DocumentRangeSemanticTokensProvider, EditorState, HoverProvider, Rope,
    RopeExt as _,
};
use gpui_kit::*;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    CompletionTriggerKind, Documentation, Hover, HoverContents, MarkedString, SemanticToken,
    SemanticTokenType, SemanticTokens, SemanticTokensLegend, TextEdit,
};
use serde_json::json;

use crate::theme;

/// Highlight names for `{{token}}` references, in legend order. The app
/// highlight theme gives them the Vue `.cm-token` colors.
const TOKEN_RESOLVED: &str = "variable.special";
const TOKEN_ENV: &str = "preproc";
const TOKEN_UNRESOLVED: &str = "label";

/// Highlight queries for `tree-sitter-graphql`, which gpui-component links
/// without any. Colors follow the Vue GraphQL editor: keywords and types in
/// the keyword color, values as in JSON, names in the text color.
const GRAPHQL_HIGHLIGHTS: &str = r#"
(comment) @comment
(description) @string
(string_value) @string
(int_value) @number
(float_value) @number
(boolean_value) @boolean
(null_value) @constant
(enum_value) @constant
(variable) @variable
(named_type (name) @type)
(fragment_name) @type
(field (name) @property)
(argument (name) @property)
(object_field (name) @property)
(field_definition (name) @property)
(input_value_definition (name) @property)
(directive "@" @keyword (name) @keyword)
(object_type_definition (name) @type)
(interface_type_definition (name) @type)
(union_type_definition (name) @type)
(enum_type_definition (name) @type)
(input_object_type_definition (name) @type)
(scalar_type_definition (name) @type)
[
  "query"
  "mutation"
  "subscription"
  "fragment"
  "on"
  "type"
  "interface"
  "union"
  "enum"
  "input"
  "scalar"
  "schema"
  "extend"
  "implements"
  "directive"
  "repeatable"
] @keyword
"#;

/// Give the linked GraphQL grammar its highlight queries. Runs once.
pub fn register_graphql() {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let registry = LanguageRegistry::singleton();
        if let Some(mut config) = registry.language("graphql") {
            config.highlights = GRAPHQL_HIGHLIGHTS.into();
            registry.register("graphql", &config);
        }
    });
}

/// The editor highlight theme in the app theme colors, as the Vue
/// CodeMirror `HighlightStyle`: property names in the text color, strings in
/// success, numbers and literals in warning, keywords and types in keyword,
/// comments muted. The editor has no background or current-line color.
pub fn highlight_theme(cx: &App) -> Option<HighlightTheme> {
    let colors = theme::colors(cx);
    let dark = colors.background.l < 0.5;
    let hex = |color: Hsla| serde_json::to_value(color).unwrap_or_default();
    let style = |color: Hsla| json!({ "color": hex(color) });
    let clear = hex(gpui_kit::transparent_black());
    let value = json!({
        "name": "Blink",
        "appearance": if dark { "dark" } else { "light" },
        "style": {
            "editor.background": clear,
            "editor.foreground": hex(colors.foreground),
            "editor.active_line.background": clear,
            "editor.line_number": hex(colors.muted_foreground),
            "editor.active_line_number": hex(colors.foreground),
            "editor.invisible": hex(colors.muted_foreground.opacity(0.4)),
            "error": hex(colors.destructive),
            "warning": hex(colors.warning),
            "info": hex(colors.info),
            "success": hex(colors.success),
            "syntax": {
                "attribute": style(colors.foreground),
                "property": style(colors.foreground),
                "variable": style(colors.foreground),
                "string": style(colors.success),
                "string.escape": style(colors.success),
                "string.special": style(colors.success),
                "number": style(colors.warning),
                "boolean": style(colors.warning),
                "constant": style(colors.warning),
                "keyword": style(colors.keyword),
                "type": style(colors.keyword),
                "comment": style(colors.muted_foreground),
                "comment.doc": style(colors.muted_foreground),
                "punctuation": style(colors.foreground),
                TOKEN_RESOLVED: style(colors.keyword),
                TOKEN_ENV: style(colors.warning),
                TOKEN_UNRESOLVED: style(colors.destructive),
            },
        },
    });
    serde_json::from_value(value).ok()
}

/// Apply [`highlight_theme`] to the GPUI Kit theme. Changes nothing when it
/// is already applied, so a theme observer can call it.
pub fn apply_highlight_theme(cx: &mut App) {
    let Some(theme) = highlight_theme(cx) else {
        return;
    };
    if *gpui_kit::component::Theme::global(cx).highlight_theme != theme {
        gpui_kit::component::Theme::global_mut(cx).highlight_theme = Arc::new(theme);
    }
}

/// Token and schema state shared with the editor providers.
#[derive(Default)]
pub struct BodyLanguage {
    tokens: RefCell<Option<InterpolationContext>>,
    schema: RefCell<Option<Arc<GraphqlSchema>>>,
}

impl BodyLanguage {
    pub fn new() -> Rc<Self> {
        Rc::new(BodyLanguage::default())
    }

    /// Returns whether the tokens changed.
    pub fn set_tokens(&self, tokens: Option<InterpolationContext>) -> bool {
        let changed = *self.tokens.borrow() != tokens;
        *self.tokens.borrow_mut() = tokens;
        changed
    }

    /// Returns whether the schema changed.
    pub fn set_schema(&self, schema: Option<Arc<GraphqlSchema>>) -> bool {
        let changed = match (&*self.schema.borrow(), &schema) {
            (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
            (None, None) => false,
            _ => true,
        };
        *self.schema.borrow_mut() = schema;
        changed
    }
}

/// `{{name}}` suggestions for the reference typed before `offset`.
pub fn token_completions(
    text: &str,
    offset: usize,
    tokens: &InterpolationContext,
) -> Option<(usize, Vec<TokenSuggestion>)> {
    let query = token_query_at(text, offset)?;
    let closing = text[offset..].starts_with("}}");
    let options = match_token_options(&token_options(Some(tokens)), &query.query);
    let items = options
        .into_iter()
        .map(|option| TokenSuggestion {
            insert: if closing {
                option.name.clone()
            } else {
                format!("{}}}}}", option.name)
            },
            info: match option.scope {
                TokenScope::Local => "Group token",
                TokenScope::Global => "Global token",
            },
            label: option.name,
            value: option.value,
        })
        .collect();
    Some((query.from, items))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSuggestion {
    pub label: String,
    pub value: String,
    pub info: &'static str,
    pub insert: String,
}

fn replace(text: &Rope, from: usize, to: usize, new_text: String) -> Option<CompletionTextEdit> {
    Some(CompletionTextEdit::Edit(TextEdit {
        range: lsp_types::Range::new(text.offset_to_position(from), text.offset_to_position(to)),
        new_text,
    }))
}

fn completion_kind(kind: CompletionKind) -> CompletionItemKind {
    match kind {
        CompletionKind::Keyword => CompletionItemKind::KEYWORD,
        CompletionKind::Field => CompletionItemKind::FIELD,
        CompletionKind::Argument => CompletionItemKind::VARIABLE,
        CompletionKind::Type => CompletionItemKind::CLASS,
        CompletionKind::EnumValue => CompletionItemKind::ENUM_MEMBER,
        CompletionKind::Directive => CompletionItemKind::KEYWORD,
    }
}

impl CompletionProvider for BodyLanguage {
    fn completions(
        &self,
        rope: &Rope,
        offset: usize,
        _trigger: CompletionContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let text = rope.to_string();
        let offset = offset.min(text.len());
        let mut items = Vec::new();
        let tokens = self.tokens.borrow();
        if let Some((from, suggestions)) = tokens
            .as_ref()
            .and_then(|tokens| token_completions(&text, offset, tokens))
        {
            items = suggestions
                .into_iter()
                .map(|suggestion| CompletionItem {
                    text_edit: replace(rope, from, offset, suggestion.insert),
                    label: suggestion.label,
                    detail: Some(suggestion.value),
                    documentation: Some(Documentation::String(suggestion.info.into())),
                    kind: Some(CompletionItemKind::VARIABLE),
                    ..Default::default()
                })
                .collect();
        } else if let Some(schema) = self.schema.borrow().as_ref() {
            let completions = graphql_completions(schema, &text, offset);
            items = completions
                .items
                .into_iter()
                .map(|item| CompletionItem {
                    text_edit: replace(rope, completions.from, offset, item.label.clone()),
                    kind: Some(completion_kind(item.kind)),
                    detail: (!item.detail.is_empty()).then_some(item.detail),
                    documentation: item.documentation.map(Documentation::String),
                    deprecated: item.deprecated.then_some(true),
                    label: item.label,
                    ..Default::default()
                })
                .collect();
        }
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        !new_text.is_empty()
            && new_text
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '{' | '.' | '!' | '$' | '@'))
    }
}

/// Start of the word that ends at `offset`: the text the menu filters on.
fn word_start(text: &str, offset: usize) -> usize {
    text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map_or(offset, |(at, _)| at)
}

/// Open the completion menu at the cursor without typing, as Ctrl+Space
/// does in CodeMirror. The editor only asks its provider after typed text.
pub fn show_completions(state: &Entity<EditorState>, window: &mut Window, cx: &mut App) {
    let (provider, text, offset) = {
        let state = state.read(cx);
        let Some(provider) = state.lsp().completion_provider.clone() else {
            return;
        };
        (provider, state.text().clone(), state.cursor())
    };
    let source = text.to_string();
    let from = word_start(&source, offset);
    let query = source[from..offset].to_string();
    let trigger = CompletionContext {
        trigger_kind: CompletionTriggerKind::INVOKED,
        trigger_character: None,
    };
    let task = provider.completions(&text, offset, trigger, window, cx);
    let state = state.downgrade();
    window
        .spawn(cx, async move |cx| {
            let items = match task.await? {
                CompletionResponse::Array(items) => items,
                CompletionResponse::List(list) => list.items,
            };
            state.update_in(cx, |state, window, cx| {
                // Skip a stale reply: the cursor moved or focus left.
                if state.cursor() == offset && state.focus_handle(cx).is_focused(window) {
                    state.present_completion_items(from, query, items, cx);
                }
            })
        })
        .detach_and_log_err(cx);
}

/// The token references in `text`, as semantic tokens in legend order.
fn semantic_tokens(
    rope: &Rope,
    text: &str,
    tokens: Option<&InterpolationContext>,
) -> Vec<SemanticToken> {
    let mut data = Vec::new();
    let (mut line, mut start) = (0, 0);
    for range in token_ranges(text, tokens) {
        let from = rope.offset_to_position(range.from);
        let to = rope.offset_to_position(range.to);
        if from.line != to.line {
            continue;
        }
        let token_type = match range.token {
            TokenState::Resolved => 0,
            TokenState::Env => 1,
            TokenState::Unresolved => 2,
        };
        let delta_line = from.line - line;
        let delta_start = if delta_line == 0 {
            from.character - start
        } else {
            from.character
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: to.character - from.character,
            token_type,
            token_modifiers_bitset: 0,
        });
        line = from.line;
        start = from.character;
    }
    data
}

impl DocumentRangeSemanticTokensProvider for BodyLanguage {
    fn legend(&self) -> SemanticTokensLegend {
        SemanticTokensLegend {
            token_types: vec![
                SemanticTokenType::new(TOKEN_RESOLVED),
                SemanticTokenType::new(TOKEN_ENV),
                SemanticTokenType::new(TOKEN_UNRESOLVED),
            ],
            token_modifiers: vec![],
        }
    }

    fn semantic_tokens(
        &self,
        rope: &Rope,
        _range: std::ops::Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<Result<SemanticTokens>> {
        let tokens = self.tokens.borrow();
        let data = match tokens.as_ref() {
            Some(tokens) => semantic_tokens(rope, &rope.to_string(), Some(tokens)),
            None => Vec::new(),
        };
        Task::ready(Ok(SemanticTokens {
            result_id: None,
            data,
        }))
    }
}

impl HoverProvider for BodyLanguage {
    fn hover(
        &self,
        rope: &Rope,
        offset: usize,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<Result<Option<Hover>>> {
        let tokens = self.tokens.borrow();
        let Some(ctx) = tokens.as_ref() else {
            return Task::ready(Ok(None));
        };
        let text = rope.to_string();
        let hover = token_ranges(&text, Some(ctx))
            .into_iter()
            .find(|range| range.from <= offset && offset < range.to)
            .map(|range| {
                let span = TokenSpan {
                    text: text[range.from..range.to].to_string(),
                    name: range.name.clone(),
                    token: Some(range.token),
                };
                Hover {
                    contents: HoverContents::Scalar(MarkedString::String(token_hint(
                        &span,
                        Some(ctx),
                    ))),
                    range: Some(lsp_types::Range::new(
                        rope.offset_to_position(range.from),
                        rope.offset_to_position(range.to),
                    )),
                }
            });
        Task::ready(Ok(hover))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    use blink_core::model::Definitions;

    fn ctx() -> InterpolationContext {
        let mut local = Definitions::new();
        local.insert("host".into(), "api.test".into());
        InterpolationContext::new(local, Definitions::new())
    }

    #[test]
    fn word_start_finds_the_word_before_the_cursor() {
        assert_eq!(word_start("{ pages { li", 12), 10);
        assert_eq!(word_start("{ pages { ", 10), 10);
        assert_eq!(word_start("{{_é_x", 7), 2);
        assert_eq!(word_start("", 0), 0);
    }

    #[test]
    fn suggests_tokens_after_open_braces() {
        let (from, items) = token_completions("{\"a\": \"{{ho", 11, &ctx()).unwrap();
        assert_eq!(from, 9);
        assert_eq!(items[0].label, "host");
        assert_eq!(items[0].insert, "host}}");
        assert_eq!(items[0].info, "Group token");
        let (_, items) = token_completions("{{h}}", 3, &ctx()).unwrap();
        assert_eq!(items[0].insert, "host");
        assert!(token_completions("{ host", 6, &ctx()).is_none());
    }

    #[test]
    fn encodes_token_references_by_line() {
        let text = "{{host}} {{nope}}\n  {{!HOME}}";
        let rope = Rope::from_str(text);
        let data = semantic_tokens(&rope, text, Some(&ctx()));
        let rows: Vec<_> = data
            .iter()
            .map(|t| (t.delta_line, t.delta_start, t.length, t.token_type))
            .collect();
        assert_eq!(rows, [(0, 0, 8, 0), (0, 9, 8, 2), (1, 2, 9, 1)]);
    }

    #[test]
    fn highlight_theme_reads_the_app_colors() {
        let theme: HighlightTheme = serde_json::from_value(json!({
            "name": "t",
            "style": { "syntax": { "variable.special": { "color": "#ff0000ff" } } },
        }))
        .unwrap();
        assert!(theme.style("variable.special").is_some());
    }
}
