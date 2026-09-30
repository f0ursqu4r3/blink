//! Port of `src/lib/ghostty.ts`.
//!
//! Parser for Ghostty colour configuration (`key = value` lines), ported
//! from term0. Only colour keys are read; other keys are ignored, so a full
//! Ghostty config or a theme file both work.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub cursor_text: String,
    pub selection_background: String,
    pub selection_foreground: String,
    /// ANSI colours 0 to 15, as lowercase `#rrggbb`.
    pub ansi: [String; 16],
}

const ANSI_COUNT: usize = 16;

/// `#rgb`, `#rrggbb`, `rgb` or `rrggbb` as lowercase `#rrggbb`.
pub fn hex_colour(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed).to_lowercase();
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => Some(format!("#{hex}")),
        3 => Some(format!(
            "#{}",
            hex.chars()
                .flat_map(|digit| [digit, digit])
                .collect::<String>()
        )),
        _ => None,
    }
}

fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

/// `^(\d+)\s*=\s*(.+)$`: the slot and the colour text.
fn palette_entry(value: &str) -> Option<(usize, &str)> {
    let digits = value.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = value[digits..].trim_start().strip_prefix('=')?.trim_start();
    if rest.is_empty() {
        return None;
    }
    // A slot too large to parse is still out of range.
    let slot = value[..digits].parse().unwrap_or(usize::MAX);
    Some((slot, rest))
}

/// Parse `text` over `base`: keys the text sets replace the base values.
/// Text with no colour key, or with any colour that is not a hex value, is
/// rejected as a whole.
pub fn parse_ghostty(text: &str, base: &Palette) -> Result<Palette, String> {
    let mut palette = base.clone();
    let mut found = 0;
    for (index, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(equals) = line.find('=') else {
            continue;
        };
        let key = line[..equals].trim();
        let value = unquote(&line[equals + 1..]);
        let place = format!("line {}", index + 1);
        if key == "palette" {
            let entry = palette_entry(value).filter(|(slot, _)| *slot < ANSI_COUNT);
            let Some((slot, colour)) = entry else {
                return Err(format!("{place}: expected palette = 0..15=#rrggbb"));
            };
            let Some(colour) = hex_colour(colour) else {
                return Err(format!("{place}: palette {slot} is not a hex colour"));
            };
            palette.ansi[slot] = colour;
            found += 1;
            continue;
        }
        let field = match key {
            "background" => &mut palette.background,
            "foreground" => &mut palette.foreground,
            "cursor-color" => &mut palette.cursor,
            "cursor-text" => &mut palette.cursor_text,
            "selection-background" => &mut palette.selection_background,
            "selection-foreground" => &mut palette.selection_foreground,
            _ => continue,
        };
        let Some(colour) = hex_colour(value) else {
            return Err(format!("{place}: {key} is not a hex colour"));
        };
        *field = colour;
        found += 1;
    }
    if found == 0 {
        return Err("no colour keys found".into());
    }
    Ok(palette)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Palette {
        Palette {
            background: "#000000".into(),
            foreground: "#ffffff".into(),
            cursor: "#ffffff".into(),
            cursor_text: "#000000".into(),
            selection_background: "#444444".into(),
            selection_foreground: "#ffffff".into(),
            ansi: std::array::from_fn(|_| "#808080".into()),
        }
    }

    #[test]
    fn normalises_hex_forms() {
        assert_eq!(hex_colour("#ABCDEF").as_deref(), Some("#abcdef"));
        assert_eq!(hex_colour("abcdef").as_deref(), Some("#abcdef"));
        assert_eq!(hex_colour("#fa0").as_deref(), Some("#ffaa00"));
    }

    #[test]
    fn rejects_other_values() {
        for value in ["", "#abcd", "red", "rgb(0,0,0)", "#gggggg"] {
            assert_eq!(hex_colour(value), None, "{value}");
        }
    }

    #[test]
    fn keys_the_text_omits_keep_the_base_value() {
        let palette =
            parse_ghostty("background = 101010\npalette = 1 = #ff0000\n", &base()).unwrap();
        assert_eq!(palette.background, "#101010");
        assert_eq!(palette.ansi[1], "#ff0000");
        assert_eq!(palette.ansi[2], "#808080");
        assert_eq!(palette.foreground, "#ffffff");
    }

    #[test]
    fn reads_a_ghostty_theme_file() {
        let text = [
            "palette = 0=#262427",
            "palette = 3=#ffc739",
            "background = #262427",
            "foreground = #fcfcfa",
            "cursor-color = #fcfcfa",
            "cursor-text = #000000",
            "selection-background = #fcfcfa",
            "selection-foreground = #262427",
        ]
        .join("\n");
        let palette = parse_ghostty(&text, &base()).unwrap();
        assert_eq!(palette.background, "#262427");
        assert_eq!(palette.foreground, "#fcfcfa");
        assert_eq!(palette.cursor, "#fcfcfa");
        assert_eq!(palette.cursor_text, "#000000");
        assert_eq!(palette.selection_background, "#fcfcfa");
        assert_eq!(palette.selection_foreground, "#262427");
        assert_eq!(palette.ansi[3], "#ffc739");
    }

    #[test]
    fn ignores_comments_blank_lines_and_other_config_keys() {
        let text = "# theme\n\nfont-family = \"Berkeley Mono\"\nfont-size = 13\nforeground = \"#eeeeee\"\n";
        assert_eq!(parse_ghostty(text, &base()).unwrap().foreground, "#eeeeee");
    }

    #[test]
    fn does_not_change_the_base_palette() {
        let base = base();
        let before = base.clone();
        let _ = parse_ghostty("palette = 0=#ffffff\nbackground = #ffffff\n", &base);
        assert_eq!(base, before);
    }

    #[test]
    fn rejects_text_with_no_colour_key() {
        assert_eq!(
            parse_ghostty("font-size = 12\n", &base()),
            Err("no colour keys found".into())
        );
        assert!(parse_ghostty("", &base()).is_err());
    }

    #[test]
    fn rejects_bad_colours_and_slots_with_the_line_number() {
        assert_eq!(
            parse_ghostty("background = #000\nforeground = white\n", &base()),
            Err("line 2: foreground is not a hex colour".into())
        );
        assert_eq!(
            parse_ghostty("palette = 16=#000000\n", &base()),
            Err("line 1: expected palette = 0..15=#rrggbb".into())
        );
        assert_eq!(
            parse_ghostty("palette = 3=zzz\n", &base()),
            Err("line 1: palette 3 is not a hex colour".into())
        );
    }

    #[test]
    fn reads_crlf_lines() {
        let palette =
            parse_ghostty("background = #111111\r\nforeground = #222222\r\n", &base()).unwrap();
        assert_eq!(
            (palette.background.as_str(), palette.foreground.as_str()),
            ("#111111", "#222222")
        );
    }
}
