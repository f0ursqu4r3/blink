//! Blink's palette on top of the GPUI Kit theme. Port of `style.css` and
//! `useTheme`: every color comes from `blink_core::theme::ThemeTokens`.

use std::path::PathBuf;

use blink_core::engine::Engine;
use blink_core::model::EnvironmentColor;
use blink_core::theme::{Rgba as TokenColor, SyntaxRole, ThemeState, ThemeTokens};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global, Hsla, Rgba, px};

/// The Blink tokens in use. Components read colors from here or from the
/// GPUI Kit theme, which `apply` keeps in step.
#[derive(Clone)]
pub struct Palette {
    pub tokens: ThemeTokens,
}

impl Global for Palette {}

pub fn color(value: TokenColor) -> Hsla {
    Rgba {
        r: value.r,
        g: value.g,
        b: value.b,
        a: value.a,
    }
    .into()
}

/// Blink semantic colors as GPUI colors.
#[derive(Clone, Copy)]
pub struct Colors {
    pub background: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub secondary: Hsla,
    pub accent: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub success: Hsla,
    pub destructive: Hsla,
    pub frame: Hsla,
    pub info: Hsla,
    pub warning: Hsla,
    pub keyword: Hsla,
    pub selection: Hsla,
    pub focus_tint: Hsla,
    pub find_match: Hsla,
    pub find_current: Hsla,
    pub find_current_foreground: Hsla,
}

impl Colors {
    fn new(tokens: &ThemeTokens) -> Self {
        Colors {
            background: color(tokens.background),
            foreground: color(tokens.foreground),
            muted: color(tokens.muted),
            muted_foreground: color(tokens.muted_foreground),
            secondary: color(tokens.secondary),
            accent: color(tokens.accent),
            primary: color(tokens.primary),
            primary_foreground: color(tokens.primary_foreground),
            border: color(tokens.border),
            input: color(tokens.input),
            success: color(tokens.success),
            destructive: color(tokens.destructive),
            frame: color(tokens.frame),
            info: color(tokens.info),
            warning: color(tokens.warning),
            keyword: color(tokens.keyword),
            selection: color(tokens.selection),
            focus_tint: color(tokens.focus_tint),
            find_match: color(tokens.find_match),
            find_current: color(tokens.find_current),
            find_current_foreground: color(tokens.find_current_foreground),
        }
    }
}

/// Colors from the active palette.
pub fn colors(cx: &App) -> Colors {
    Colors::new(&cx.global::<Palette>().tokens)
}

/// HTTP method color: GET success, POST warning, PUT/PATCH info, DELETE
/// error, HEAD/OPTIONS keyword.
pub fn method_color(method: &str, cx: &App) -> Hsla {
    color(cx.global::<Palette>().tokens.method_color(method))
}

pub fn environment_color(value: EnvironmentColor, cx: &App) -> Hsla {
    color(cx.global::<Palette>().tokens.environment_color(value))
}

pub fn syntax_color(role: SyntaxRole, cx: &App) -> Hsla {
    color(cx.global::<Palette>().tokens.syntax_color(role))
}

/// Status code color: 4xx and 5xx error, 3xx warning, else success.
pub fn status_color(status: u16, cx: &App) -> Hsla {
    let colors = colors(cx);
    if status >= 400 {
        colors.destructive
    } else if status >= 300 {
        colors.warning
    } else {
        colors.success
    }
}

/// System sans-serif for chrome, as `--font-sans`.
pub const SANS: &str = ".SystemUIFont";
/// `ui-monospace` resolves to SF Mono on macOS.
pub const MONO: &str = "SF Mono";
/// Body text: 0.8125rem of a 16 px root.
pub const FONT_SIZE: f32 = 13.0;

/// The app theme setting. The Vue app kept it in localStorage; the native
/// app keeps it in `theme.json` in the data directory.
pub struct AppTheme {
    pub state: ThemeState,
    path: PathBuf,
}

impl Global for AppTheme {}

impl AppTheme {
    fn stored(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }

    /// Store `text`, or remove the file for the default theme.
    pub fn write(&self, text: Option<String>) -> Result<(), String> {
        match text {
            Some(text) => std::fs::write(&self.path, text).map_err(|error| error.to_string()),
            None => match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    Err(error.to_string())
                }
                _ => Ok(()),
            },
        }
    }
}

/// Load the saved theme and apply it.
pub fn init(engine: &Engine, cx: &mut App) {
    let path = engine.paths().data_dir.join(THEME_FILE);
    let state = ThemeState::new(std::fs::read_to_string(&path).ok().as_deref());
    let tokens = state.tokens.clone();
    cx.set_global(AppTheme { state, path });
    apply(tokens, cx);
}

/// Change the theme state, then show its tokens.
pub fn update_theme<R>(cx: &mut App, change: impl FnOnce(&mut AppTheme) -> R) -> R {
    let result = change(cx.global_mut::<AppTheme>());
    let tokens = cx.global::<AppTheme>().state.tokens.clone();
    apply(tokens, cx);
    result
}

/// Re-read the stored theme, as the Vue app did when storage changed.
pub fn reload_theme(cx: &mut App) {
    update_theme(cx, |theme| {
        let stored = theme.stored();
        theme.state.reload(stored.as_deref());
    });
}

/// The saved theme name, shown at the end of the status bar.
pub fn theme_name(cx: &App) -> String {
    cx.global::<AppTheme>().state.name().to_string()
}

const THEME_FILE: &str = "theme.json";

/// Install `tokens` as the palette and project it onto every GPUI Kit color.
pub fn apply(tokens: ThemeTokens, cx: &mut App) {
    let c = Colors::new(&tokens);
    let dark = matches!(tokens.scheme, blink_core::theme::ColorScheme::Dark);
    let scrollbar = (
        color(tokens.scrollbar_thumb),
        color(tokens.scrollbar_thumb_hover),
    );
    cx.set_global(Palette { tokens });
    let mode = if dark { ThemeMode::Dark } else { ThemeMode::Light };
    if Theme::global(cx).mode != mode {
        Theme::change(mode, None, cx);
    }
    Theme::update(cx, |theme| {
        theme.font_family = SANS.into();
        theme.mono_font_family = MONO.into();
        theme.font_size = px(FONT_SIZE);
        theme.mono_font_size = px(12.0);
        // 4 px for controls and tooltips, 8 px for cards, menus, and dialogs.
        theme.radius = px(4.0);
        theme.radius_lg = px(8.0);
        theme.shadow = true;
        // VS Code focus: no ring, a subtle tint and the focused border.
        theme.focus_ring = false;

        let t = &mut theme.colors;
        t.background = c.background;
        t.foreground = c.foreground;
        t.border = c.border;
        t.input = c.input;
        t.ring = c.primary;
        t.caret = c.foreground;
        t.selection = c.selection;
        t.muted = c.muted;
        t.muted_foreground = c.muted_foreground;
        t.accent = c.accent;
        t.accent_foreground = c.foreground;
        t.popover = c.secondary;
        t.popover_foreground = c.foreground;
        t.overlay = c.frame.opacity(0.6);
        t.window_border = c.border;

        t.primary = c.primary;
        t.primary_hover = c.primary.opacity(0.9);
        t.primary_active = c.primary.opacity(0.8);
        t.primary_foreground = c.primary_foreground;
        t.secondary = c.secondary;
        t.secondary_hover = c.accent;
        t.secondary_active = c.accent;
        t.secondary_foreground = c.foreground;

        t.button = c.secondary;
        t.button_hover = c.accent;
        t.button_active = c.accent;
        t.button_foreground = c.foreground;
        t.button_primary = c.primary;
        t.button_primary_hover = c.primary.opacity(0.9);
        t.button_primary_active = c.primary.opacity(0.8);
        t.button_primary_foreground = c.primary_foreground;
        t.button_secondary = c.secondary;
        t.button_secondary_hover = c.accent;
        t.button_secondary_active = c.accent;
        t.button_secondary_foreground = c.foreground;
        t.button_danger = c.destructive;
        t.button_danger_hover = c.destructive.opacity(0.9);
        t.button_danger_active = c.destructive.opacity(0.8);
        t.button_danger_foreground = c.background;

        t.danger = c.destructive;
        t.danger_hover = c.destructive.opacity(0.9);
        t.danger_active = c.destructive.opacity(0.8);
        t.danger_foreground = c.background;
        t.success = c.success;
        t.success_hover = c.success.opacity(0.9);
        t.success_active = c.success.opacity(0.8);
        t.success_foreground = c.background;
        t.warning = c.warning;
        t.warning_hover = c.warning.opacity(0.9);
        t.warning_active = c.warning.opacity(0.8);
        t.warning_foreground = c.background;
        t.info = c.info;
        t.info_hover = c.info.opacity(0.9);
        t.info_active = c.info.opacity(0.8);
        t.info_foreground = c.background;

        t.link = c.info;
        t.link_hover = c.info;
        t.link_active = c.info;

        t.list = c.background;
        t.list_even = c.background;
        t.list_head = c.muted;
        t.list_hover = c.focus_tint;
        t.list_active = c.accent;
        t.list_active_border = c.primary;
        t.table = c.background;
        t.table_even = c.background;
        t.table_head = c.muted;
        t.table_head_foreground = c.muted_foreground;
        t.table_hover = c.focus_tint;
        t.table_active = c.accent;
        t.table_active_border = c.primary;
        t.table_row_border = c.border;

        t.tab_bar = c.muted;
        t.tab = c.muted;
        t.tab_foreground = c.muted_foreground;
        t.tab_active = c.background;
        t.tab_active_foreground = c.foreground;
        t.tab_bar_segmented = c.muted;

        t.title_bar = c.frame;
        t.title_bar_border = c.frame;
        t.status_bar = c.frame;
        t.status_bar_border = c.frame;
        t.sidebar = c.background;
        t.sidebar_foreground = c.foreground;
        t.sidebar_border = c.border;
        t.sidebar_accent = c.accent;
        t.sidebar_accent_foreground = c.foreground;
        t.sidebar_primary = c.primary;
        t.sidebar_primary_foreground = c.primary_foreground;

        t.switch = c.input;
        t.switch_thumb = c.foreground;
        t.slider_bar = c.primary;
        t.slider_thumb = c.foreground;
        t.progress_bar = c.primary;
        t.skeleton = c.muted;
        t.drag_border = c.primary;
        t.drop_target = c.primary.opacity(0.15);
        t.accordion = c.background;
        t.group_box = c.muted;
        t.group_box_foreground = c.foreground;
        t.description_list_label = c.muted;
        t.description_list_label_foreground = c.muted_foreground;
        t.scrollbar = gpui_kit::transparent_black();
        t.scrollbar_thumb = scrollbar.0;
        t.scrollbar_thumb_hover = scrollbar.1;
    });
}
