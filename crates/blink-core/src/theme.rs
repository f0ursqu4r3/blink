//! Port of `src/lib/theme.ts`, the token rules of `src/style.css`, and the
//! theme state of `src/composables/useTheme.ts`.
//!
//! The web app writes `--term-*` properties and lets CSS derive every token
//! with `color-mix(in oklch, ...)`. Here the same mixes run in Rust, so the
//! UI reads plain sRGB colors from `ThemeTokens`.

use serde::{Deserialize, Serialize};

use crate::ghostty::{Palette, parse_ghostty};
use crate::model::EnvironmentColor;

pub const THEME_KEY: &str = "blink.theme";

/// ANSI slot 1 to 6 used as the accent.
pub type AccentSlot = u8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeSetting {
    pub name: String,
    pub text: String,
    pub accent: AccentSlot,
}

pub const ACCENT_SLOTS: [(AccentSlot, &str); 6] = [
    (1, "Red"),
    (2, "Green"),
    (3, "Yellow"),
    (4, "Blue"),
    (5, "Magenta"),
    (6, "Cyan"),
];

/// Accent slot for a new Ghostty theme: Blue.
pub const DEFAULT_ACCENT: AccentSlot = 4;

/// Hex approximation of the default amber theme. Only used for keys a
/// Ghostty text omits; with no theme the oklch tokens of `default_tokens`
/// apply.
pub fn base_palette() -> Palette {
    let ansi = [
        "#151410", "#f48f79", "#92d193", "#eeb93c", "#9ec3e8", "#c9a3e0", "#8fd3d0", "#e8e3d6",
        "#5a574e", "#f7a898", "#aee0ae", "#f5cc6a", "#b8d4f0", "#d8bce9", "#aee2df", "#ffffff",
    ];
    Palette {
        background: "#151410".into(),
        foreground: "#e8e3d6".into(),
        cursor: "#e8e3d6".into(),
        cursor_text: "#151410".into(),
        selection_background: "#eeb93c".into(),
        selection_foreground: "#151410".into(),
        ansi: ansi.map(String::from),
    }
}

pub fn parse_theme(setting: &ThemeSetting) -> Result<Palette, String> {
    parse_ghostty(&setting.text, &base_palette())
}

fn channel(hex: &str, start: usize) -> f32 {
    hex.get(start..start + 2)
        .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        .unwrap_or(0) as f32
        / 255.0
}

/// WCAG relative luminance of a `#rrggbb` colour.
pub fn luminance(hex: &str) -> f64 {
    let [r, g, b] = [1, 3, 5].map(|start| {
        let channel = channel(hex, start) as f64;
        if channel <= 0.03928 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// An sRGB color, each channel from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    /// `#rrggbb` or `#rrggbbaa`.
    pub fn from_hex(hex: &str) -> Option<Rgba> {
        let digits = hex.strip_prefix('#').unwrap_or(hex);
        if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let at = |start: usize| channel(digits, start);
        Some(Rgba {
            r: at(0),
            g: at(2),
            b: at(4),
            a: if digits.len() == 8 { at(6) } else { 1.0 },
        })
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn to_hex(self) -> String {
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        let rgb = format!(
            "#{:02x}{:02x}{:02x}",
            byte(self.r),
            byte(self.g),
            byte(self.b)
        );
        if byte(self.a) == 255 {
            rgb
        } else {
            format!("{rgb}{:02x}", byte(self.a))
        }
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba { a, ..self }
    }

    /// `color-mix(in oklch, self p%, transparent)`.
    pub fn fade(self, percent: f32) -> Rgba {
        self.with_alpha(self.a * percent / 100.0)
    }
}

/// A color in OKLCH. `h` is None when the hue is powerless (gray).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: Option<f32>,
    pub alpha: f32,
}

/// `oklch(l c h / alpha)` as written in style.css.
pub const fn oklch(l: f32, c: f32, h: f32) -> Oklch {
    Oklch {
        l,
        c,
        h: Some(h),
        alpha: 1.0,
    }
}

fn to_linear(value: f64) -> f64 {
    if value.abs() <= 0.04045 {
        value / 12.92
    } else {
        value.signum() * ((value.abs() + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(value: f64) -> f64 {
    if value.abs() <= 0.0031308 {
        value * 12.92
    } else {
        value.signum() * (1.055 * value.abs().powf(1.0 / 2.4) - 0.055)
    }
}

impl Oklch {
    pub fn from_rgba(color: Rgba) -> Oklch {
        let [r, g, b] = [color.r, color.g, color.b].map(|c| to_linear(c as f64));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        let lightness = 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s;
        let a = 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s;
        let b = 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s;
        let chroma = (a * a + b * b).sqrt();
        // Achromatic: the hue is powerless and does not take part in mixes.
        let hue = (chroma > 4e-6).then(|| b.atan2(a).to_degrees().rem_euclid(360.0) as f32);
        Oklch {
            l: lightness as f32,
            c: chroma as f32,
            h: hue,
            alpha: color.a,
        }
    }

    /// sRGB, with out-of-gamut channels clipped.
    pub fn to_rgba(self) -> Rgba {
        let hue = self.h.unwrap_or(0.0).to_radians() as f64;
        let (lightness, chroma) = (self.l as f64, self.c as f64);
        let a = chroma * hue.cos();
        let b = chroma * hue.sin();
        let l = (lightness + 0.3963377774 * a + 0.2158037573 * b).powi(3);
        let m = (lightness - 0.1055613458 * a - 0.0638541728 * b).powi(3);
        let s = (lightness - 0.0894841775 * a - 1.2914855480 * b).powi(3);
        let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
        let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
        let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
        let [r, g, b] = [r, g, b].map(|c| from_linear(c).clamp(0.0, 1.0) as f32);
        Rgba {
            r,
            g,
            b,
            a: self.alpha.clamp(0.0, 1.0),
        }
    }
}

/// `color-mix(in oklch, a p%, b)`: premultiplied alpha, shorter hue arc.
pub fn mix_oklch(a: Rgba, percent: f32, b: Rgba) -> Rgba {
    let (x, y) = (Oklch::from_rgba(a), Oklch::from_rgba(b));
    let p = percent / 100.0;
    let q = 1.0 - p;
    let alpha = x.alpha * p + y.alpha * q;
    if alpha <= 0.0 {
        return Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
    }
    let premultiplied = |u: f32, v: f32| (u * x.alpha * p + v * y.alpha * q) / alpha;
    let hue = match (x.h, y.h) {
        (Some(h1), Some(h2)) => {
            let mut delta = h2 - h1;
            if delta > 180.0 {
                delta -= 360.0;
            } else if delta < -180.0 {
                delta += 360.0;
            }
            Some((h1 + delta * q).rem_euclid(360.0))
        }
        (Some(h), None) | (None, Some(h)) => Some(h),
        (None, None) => None,
    };
    Oklch {
        l: premultiplied(x.l, y.l),
        c: premultiplied(x.c, y.c),
        h: hue,
        alpha,
    }
    .to_rgba()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
}

/// The CSS tokens of style.css as sRGB colors.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeTokens {
    pub scheme: ColorScheme,
    pub background: Rgba,
    pub foreground: Rgba,
    pub muted: Rgba,
    pub muted_foreground: Rgba,
    pub secondary: Rgba,
    pub popover: Rgba,
    pub popover_foreground: Rgba,
    pub accent: Rgba,
    pub primary: Rgba,
    pub primary_foreground: Rgba,
    pub border: Rgba,
    pub input: Rgba,
    pub ring: Rgba,
    pub success: Rgba,
    pub destructive: Rgba,
    pub frame: Rgba,
    pub info: Rgba,
    pub warning: Rgba,
    pub keyword: Rgba,
    /// Text selection background.
    pub selection: Rgba,
    /// `--shadow-menu` color; see `MENU_SHADOW_*` for its geometry.
    pub shadow_menu: Rgba,
    /// Tint over a focused control's own background.
    pub focus_tint: Rgba,
    /// Find matches in the response body.
    pub find_match: Rgba,
    pub find_current: Rgba,
    /// Text on the current find match.
    pub find_current_foreground: Rgba,
    pub scrollbar_thumb: Rgba,
    pub scrollbar_thumb_hover: Rgba,
    pub scrollbar_thumb_active: Rgba,
}

/// `--shadow-menu: 0 4px 12px`, in pixels.
pub const MENU_SHADOW_OFFSET_Y: f32 = 4.0;
pub const MENU_SHADOW_BLUR: f32 = 12.0;

/// Syntax color roles of the highlight.js rules in style.css.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxRole {
    /// Attributes, properties, variables, parameters, and plain text.
    Plain,
    /// Strings and regular expressions.
    String,
    /// Numbers, literals, and symbols.
    Number,
    /// Keywords, tags, names, and types.
    Keyword,
    /// Titles and built-ins.
    Title,
    /// Comments and meta.
    Comment,
}

struct CoreTokens {
    scheme: ColorScheme,
    background: Rgba,
    foreground: Rgba,
    muted: Rgba,
    muted_foreground: Rgba,
    secondary: Rgba,
    accent: Rgba,
    primary: Rgba,
    primary_foreground: Rgba,
    border: Rgba,
    input: Rgba,
    success: Rgba,
    destructive: Rgba,
    frame: Rgba,
    info: Rgba,
    warning: Rgba,
    keyword: Rgba,
    selection: Rgba,
}

impl ThemeTokens {
    fn from_core(core: CoreTokens) -> ThemeTokens {
        ThemeTokens {
            scheme: core.scheme,
            background: core.background,
            foreground: core.foreground,
            muted: core.muted,
            muted_foreground: core.muted_foreground,
            secondary: core.secondary,
            popover: core.secondary,
            popover_foreground: core.foreground,
            accent: core.accent,
            primary: core.primary,
            primary_foreground: core.primary_foreground,
            border: core.border,
            input: core.input,
            ring: core.primary,
            success: core.success,
            destructive: core.destructive,
            frame: core.frame,
            info: core.info,
            warning: core.warning,
            keyword: core.keyword,
            selection: core.selection,
            shadow_menu: core.frame.fade(60.0),
            focus_tint: core.foreground.fade(10.0),
            find_match: core.warning.fade(30.0),
            find_current: core.warning.fade(70.0),
            find_current_foreground: core.background,
            scrollbar_thumb: core.foreground.fade(20.0),
            scrollbar_thumb_hover: core.foreground.fade(32.0),
            scrollbar_thumb_active: core.foreground.fade(42.0),
        }
    }

    /// The default amber theme of style.css. It has a dark scheme only.
    pub fn default_tokens() -> ThemeTokens {
        let c = |value: Oklch| value.to_rgba();
        let primary = oklch(0.82, 0.145, 85.0);
        ThemeTokens::from_core(CoreTokens {
            scheme: ColorScheme::Dark,
            background: c(oklch(0.17, 0.006, 100.0)),
            foreground: c(oklch(0.91, 0.016, 90.0)),
            muted: c(oklch(0.2, 0.006, 100.0)),
            muted_foreground: c(oklch(0.66, 0.012, 95.0)),
            secondary: c(oklch(0.24, 0.008, 100.0)),
            accent: c(oklch(0.29, 0.01, 100.0)),
            primary: c(primary),
            primary_foreground: c(oklch(0.2, 0.016, 85.0)),
            border: c(oklch(0.31, 0.01, 100.0)),
            input: c(oklch(0.4, 0.012, 100.0)),
            success: c(oklch(0.8, 0.1, 145.0)),
            destructive: c(oklch(0.77, 0.13, 30.0)),
            frame: c(oklch(0.13, 0.005, 100.0)),
            info: c(oklch(0.8, 0.07, 240.0)),
            warning: c(oklch(0.8, 0.11, 70.0)),
            keyword: c(oklch(0.76, 0.09, 300.0)),
            selection: c(Oklch {
                alpha: 0.25,
                ..primary
            }),
        })
    }

    /// Every token from a Ghostty palette, as the `data-theme='ghostty'`
    /// rules of style.css derive them.
    pub fn from_palette(palette: &Palette, accent: AccentSlot) -> ThemeTokens {
        let hex = |value: &str| {
            Rgba::from_hex(value).unwrap_or(Rgba {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            })
        };
        let bg = hex(&palette.background);
        let fg = hex(&palette.foreground);
        let term = |slot: usize| hex(&palette.ansi[slot]);
        let black = Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        ThemeTokens::from_core(CoreTokens {
            scheme: if luminance(&palette.background) > 0.5 {
                ColorScheme::Light
            } else {
                ColorScheme::Dark
            },
            background: bg,
            foreground: fg,
            muted: mix_oklch(fg, 5.0, bg),
            secondary: mix_oklch(fg, 9.0, bg),
            accent: mix_oklch(fg, 14.0, bg),
            border: mix_oklch(fg, 18.0, bg),
            input: mix_oklch(fg, 28.0, bg),
            muted_foreground: mix_oklch(fg, 64.0, bg),
            frame: mix_oklch(bg, 78.0, black),
            primary: term((accent as usize).min(15)),
            primary_foreground: bg,
            success: term(2),
            destructive: term(1),
            info: term(4),
            warning: term(3),
            keyword: term(5),
            selection: hex(&palette.selection_background).fade(35.0),
        })
    }

    /// The color of an HTTP method name. Other methods use the foreground.
    pub fn method_color(&self, method: &str) -> Rgba {
        match method {
            "GET" => self.success,
            "POST" => self.warning,
            "PUT" | "PATCH" => self.info,
            "DELETE" => self.destructive,
            "HEAD" | "OPTIONS" => self.keyword,
            _ => self.foreground,
        }
    }

    pub fn environment_color(&self, color: EnvironmentColor) -> Rgba {
        match color {
            EnvironmentColor::Destructive => self.destructive,
            EnvironmentColor::Warning => self.warning,
            EnvironmentColor::Success => self.success,
            EnvironmentColor::Info => self.info,
            EnvironmentColor::Keyword => self.keyword,
        }
    }

    pub fn syntax_color(&self, role: SyntaxRole) -> Rgba {
        match role {
            SyntaxRole::Plain => self.foreground,
            SyntaxRole::String => self.success,
            SyntaxRole::Number => self.warning,
            SyntaxRole::Keyword => self.keyword,
            SyntaxRole::Title => self.info,
            SyntaxRole::Comment => self.muted_foreground,
        }
    }
}

/// Tokens for a theme setting, or the default tokens with None.
pub fn theme_tokens(palette: Option<&Palette>, accent: AccentSlot) -> ThemeTokens {
    match palette {
        Some(palette) => ThemeTokens::from_palette(palette, accent),
        None => ThemeTokens::default_tokens(),
    }
}

fn valid_setting(value: &serde_json::Value) -> Option<ThemeSetting> {
    let setting = value.as_object()?;
    let name = setting.get("name")?.as_str()?;
    let text = setting.get("text")?.as_str()?;
    let accent = setting.get("accent")?.as_f64()?;
    let (accent, _) = ACCENT_SLOTS
        .iter()
        .find(|(slot, _)| *slot as f64 == accent)?;
    Some(ThemeSetting {
        name: name.into(),
        text: text.into(),
        accent: *accent,
    })
}

/// The stored theme, or None (default) when nothing valid is stored.
pub fn load_theme(raw: Option<&str>) -> Option<ThemeSetting> {
    let value: serde_json::Value = serde_json::from_str(raw?).ok()?;
    let setting = valid_setting(&value)?;
    parse_theme(&setting).is_ok().then_some(setting)
}

/// The text to store, or None to remove the stored theme.
pub fn save_theme(setting: Option<&ThemeSetting>) -> Option<String> {
    setting.map(|setting| serde_json::to_string(setting).unwrap_or_default())
}

pub const STORAGE_FAILURE: &str = "Theme not saved. Blink cannot write the theme file.";

/// The app theme: the saved setting, an unsaved preview, and the tokens
/// shown now.
#[derive(Debug, Clone)]
pub struct ThemeState {
    saved: Option<ThemeSetting>,
    /// Theme shown now: the saved theme, or an unsaved preview.
    pub draft: Option<ThemeSetting>,
    /// Why the draft text does not parse, or "".
    pub error: String,
    /// Why the last commit did not reach storage, or "".
    pub save_error: String,
    /// The tokens shown now. An invalid draft keeps the last valid ones.
    pub tokens: ThemeTokens,
}

impl ThemeState {
    /// Show the stored theme (raw stored text, as `load_theme` reads it).
    pub fn new(stored: Option<&str>) -> ThemeState {
        let saved = load_theme(stored);
        let mut state = ThemeState {
            saved: saved.clone(),
            draft: saved,
            error: String::new(),
            save_error: String::new(),
            tokens: ThemeTokens::default_tokens(),
        };
        state.revert();
        state
    }

    /// Saved theme name for the status bar.
    pub fn name(&self) -> &str {
        self.saved.as_ref().map_or("Blink", |setting| &setting.name)
    }

    pub fn preview(&mut self, next: Option<ThemeSetting>) {
        self.draft = next;
        self.save_error.clear();
        let Some(next) = &self.draft else {
            self.error.clear();
            self.tokens = ThemeTokens::default_tokens();
            return;
        };
        match parse_theme(next) {
            Ok(palette) => {
                self.error.clear();
                self.tokens = ThemeTokens::from_palette(&palette, next.accent);
            }
            Err(error) => self.error = error,
        }
    }

    /// Save the draft with `write` (`save_theme` output; None removes).
    /// Returns the parse or save error, or "".
    pub fn commit(&mut self, write: impl FnOnce(Option<String>) -> Result<(), String>) -> String {
        if !self.error.is_empty() {
            return self.error.clone();
        }
        if self.draft == self.saved {
            return String::new();
        }
        self.saved = self.draft.clone();
        match write(save_theme(self.draft.as_ref())) {
            Ok(()) => self.save_error.clear(),
            Err(_) => self.save_error = STORAGE_FAILURE.into(),
        }
        self.save_error.clone()
    }

    pub fn revert(&mut self) {
        self.preview(self.saved.clone());
    }

    /// Re-read storage and show the saved theme.
    pub fn reload(&mut self, stored: Option<&str>) {
        self.saved = load_theme(stored);
        self.revert();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light() -> ThemeSetting {
        ThemeSetting {
            name: "Paper".into(),
            text: "background = #ffffff\nforeground = #111111\npalette = 4=#0055ff\n".into(),
            accent: 4,
        }
    }

    fn paper() -> ThemeSetting {
        ThemeSetting {
            name: "Paper".into(),
            text: "background = #ffffff\nforeground = #111111".into(),
            accent: 3,
        }
    }

    fn close(a: Rgba, b: Rgba) -> bool {
        [a.r - b.r, a.g - b.g, a.b - b.b, a.a - b.a]
            .iter()
            .all(|d| d.abs() < 0.003)
    }

    #[test]
    fn fills_keys_the_text_omits_from_the_base_palette() {
        let palette = parse_theme(&ThemeSetting {
            name: "x".into(),
            text: "background = #ffffff".into(),
            accent: 3,
        })
        .unwrap();
        assert_eq!(palette.foreground, base_palette().foreground);
        assert_eq!(palette.ansi[3], base_palette().ansi[3]);
    }

    #[test]
    fn measures_relative_luminance() {
        assert!((luminance("#ffffff") - 1.0).abs() < 1e-6);
        assert!(luminance("#000000").abs() < 1e-6);
    }

    #[test]
    fn writes_term_colors_the_accent_slot_and_the_colour_scheme() {
        let palette = parse_theme(&light()).unwrap();
        let tokens = ThemeTokens::from_palette(&palette, 4);
        assert_eq!(tokens.background.to_hex(), "#ffffff");
        assert_eq!(tokens.foreground.to_hex(), "#111111");
        assert_eq!(tokens.primary.to_hex(), "#0055ff");
        assert_eq!(tokens.info.to_hex(), "#0055ff");
        assert_eq!(tokens.scheme, ColorScheme::Light);
        assert_eq!(
            ThemeTokens::from_palette(&base_palette(), 3).scheme,
            ColorScheme::Dark
        );
    }

    #[test]
    fn mixes_in_oklch() {
        let white = Rgba::from_hex("#ffffff").unwrap();
        let black = Rgba::from_hex("#000000").unwrap();
        // Gray mixes keep a gray: no hue enters.
        let gray = mix_oklch(white, 50.0, black);
        assert!((gray.r - gray.g).abs() < 1e-4 && (gray.g - gray.b).abs() < 1e-4);
        // L 0.5 in OKLab is about #636363.
        assert_eq!(gray.to_hex(), "#636363");
        let red = Rgba::from_hex("#ff0000").unwrap();
        assert!(close(mix_oklch(red, 100.0, black), red));
        assert!(close(mix_oklch(red, 0.0, black), black));
    }

    #[test]
    fn converts_oklch_round_trip() {
        for hex in ["#151410", "#e8e3d6", "#eeb93c", "#0055ff", "#8fd3d0"] {
            let color = Rgba::from_hex(hex).unwrap();
            assert_eq!(Oklch::from_rgba(color).to_rgba().to_hex(), hex);
        }
    }

    #[test]
    fn derives_the_default_tokens() {
        let tokens = ThemeTokens::default_tokens();
        assert_eq!(tokens.scheme, ColorScheme::Dark);
        assert_eq!(tokens.popover, tokens.secondary);
        assert_eq!(tokens.ring, tokens.primary);
        assert!((tokens.selection.a - 0.25).abs() < 1e-6);
        // style.css oklch values; base_palette holds rougher hex copies.
        assert_eq!(tokens.background.to_hex(), "#100f0d");
        assert_eq!(tokens.foreground.to_hex(), "#e5e1d6");
        assert_eq!(tokens.primary.to_hex(), "#efbc43");
        assert_eq!(tokens.method_color("GET"), tokens.success);
        assert_eq!(tokens.method_color("PATCH"), tokens.info);
        assert_eq!(tokens.method_color("PURGE"), tokens.foreground);
    }

    #[test]
    fn derives_ghostty_surfaces_from_the_foreground_and_background() {
        let palette = parse_theme(&light()).unwrap();
        let tokens = ThemeTokens::from_palette(&palette, 4);
        let bg = Oklch::from_rgba(tokens.background).l;
        let fg = Oklch::from_rgba(tokens.foreground).l;
        let at = |percent: f32| bg + (fg - bg) * percent / 100.0;
        for (token, percent) in [
            (tokens.muted, 5.0),
            (tokens.secondary, 9.0),
            (tokens.accent, 14.0),
            (tokens.border, 18.0),
            (tokens.input, 28.0),
            (tokens.muted_foreground, 64.0),
        ] {
            assert!((Oklch::from_rgba(token).l - at(percent)).abs() < 0.003);
        }
        assert!((Oklch::from_rgba(tokens.frame).l - bg * 0.78).abs() < 0.003);
        assert_eq!(tokens.selection.a, 0.35);
        assert_eq!(tokens.primary_foreground, tokens.background);
    }

    #[test]
    fn round_trips_a_setting_through_storage() {
        let stored = save_theme(Some(&light()));
        assert_eq!(load_theme(stored.as_deref()), Some(light()));
        assert_eq!(save_theme(None), None);
        assert_eq!(load_theme(None), None);
    }

    #[test]
    fn loads_the_default_for_corrupt_stored_values() {
        let text = light().text;
        for raw in [
            "not json".to_string(),
            "[]".into(),
            serde_json::json!({ "name": "x", "text": text, "accent": 9 }).to_string(),
            serde_json::json!({ "name": "x", "text": "foreground = white", "accent": 3 })
                .to_string(),
            serde_json::json!({ "name": 1, "text": text, "accent": 3 }).to_string(),
        ] {
            assert_eq!(load_theme(Some(&raw)), None, "{raw}");
        }
    }

    // Ported from useTheme.test.ts.
    #[test]
    fn applies_the_stored_theme_when_created() {
        let stored = save_theme(Some(&paper()));
        let state = ThemeState::new(stored.as_deref());
        assert_eq!(state.name(), "Paper");
        assert_eq!(state.tokens.background.to_hex(), "#ffffff");
    }

    #[test]
    fn previews_then_reverts_to_the_default() {
        let mut state = ThemeState::new(None);
        state.preview(Some(paper()));
        assert_eq!(state.tokens.background.to_hex(), "#ffffff");
        state.revert();
        assert_eq!(state.tokens, ThemeTokens::default_tokens());
        assert_eq!(state.draft, None);
    }

    #[test]
    fn keeps_the_last_valid_colors_while_the_text_is_invalid() {
        let mut state = ThemeState::new(None);
        state.preview(Some(paper()));
        state.preview(Some(ThemeSetting {
            text: "foreground = white".into(),
            ..paper()
        }));
        assert_eq!(state.error, "line 1: foreground is not a hex colour");
        assert_eq!(state.tokens.background.to_hex(), "#ffffff");
        let mut wrote = false;
        assert_eq!(
            state.commit(|_| {
                wrote = true;
                Ok(())
            }),
            "line 1: foreground is not a hex colour"
        );
        assert!(!wrote);
    }

    #[test]
    fn commits_the_draft_to_storage() {
        let mut state = ThemeState::new(None);
        let mut stored: Option<String> = None;
        state.preview(Some(paper()));
        assert_eq!(
            state.commit(|text| {
                stored = text;
                Ok(())
            }),
            ""
        );
        assert_eq!(load_theme(stored.as_deref()), Some(paper()));
        assert_eq!(state.name(), "Paper");
        state.preview(None);
        assert_eq!(
            state.commit(|text| {
                stored = text;
                Ok(())
            }),
            ""
        );
        assert_eq!(stored, None);
        assert_eq!(state.name(), "Blink");
    }

    #[test]
    fn reports_a_storage_failure_and_keeps_the_theme_for_the_session() {
        let mut state = ThemeState::new(None);
        state.preview(Some(paper()));
        assert_eq!(state.commit(|_| Err("quota".into())), STORAGE_FAILURE);
        assert_eq!(state.save_error, STORAGE_FAILURE);
        state.revert();
        assert_eq!(state.tokens.background.to_hex(), "#ffffff");
    }

    #[test]
    fn commits_without_writing_when_the_draft_has_not_changed() {
        let mut state = ThemeState::new(None);
        assert_eq!(state.commit(|_| Err("no storage".into())), "");
    }
}
