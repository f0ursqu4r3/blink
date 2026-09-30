//! Port of `src/lib/shortcut.ts`.

/// True on macOS, where "mod" is ⌘.
pub const IS_MAC: bool = cfg!(target_os = "macos");

// "ctrl" is the Control key on every platform; "mod" is ⌘ on macOS.
const MODIFIERS: [&str; 4] = ["mod", "ctrl", "shift", "alt"];
// VS Code order: ⌃⌥⇧⌘ on macOS, Ctrl+Alt+Shift elsewhere.
const MAC_ORDER: [&str; 4] = ["ctrl", "alt", "shift", "mod"];
const WORD_ORDER: [&str; 4] = ["mod", "ctrl", "alt", "shift"];

fn mac_symbol(key: &str) -> Option<&'static str> {
    Some(match key {
        "ctrl" => "⌃",
        "tab" => "Tab",
        "alt" => "⌥",
        "shift" => "⇧",
        "mod" => "⌘",
        "enter" => "↵",
        "esc" => "Esc",
        _ => return None,
    })
}

fn word(key: &str) -> Option<&'static str> {
    Some(match key {
        "ctrl" => "Ctrl",
        "tab" => "Tab",
        "alt" => "Alt",
        "shift" => "Shift",
        "mod" => "Ctrl",
        "enter" => "Enter",
        "esc" => "Esc",
        _ => return None,
    })
}

/// A shortcut hint for menus, such as "⇧⌘D" on macOS and "Ctrl+Shift+D"
/// elsewhere. Keys: "mod", "ctrl", "shift", "alt", "enter", "esc", "tab", or one
/// character. Pass `IS_MAC` for the running platform.
pub fn shortcut_label<S: AsRef<str>>(keys: &[S], mac: bool) -> String {
    let order = if mac { MAC_ORDER } else { WORD_ORDER };
    let position = |key: &str| order.iter().position(|item| *item == key);
    let mut modifiers: Vec<&str> = keys
        .iter()
        .map(AsRef::as_ref)
        .filter(|key| MODIFIERS.contains(key))
        .collect();
    modifiers.sort_by_key(|key| position(key));
    let others = keys
        .iter()
        .map(AsRef::as_ref)
        .filter(|key| !MODIFIERS.contains(key));
    modifiers
        .into_iter()
        .chain(others)
        .map(|key| {
            let name = if mac { mac_symbol(key) } else { word(key) };
            name.map_or_else(|| key.to_uppercase(), str::to_string)
        })
        .collect::<Vec<_>>()
        .join(if mac { "" } else { "+" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_symbols_in_vs_code_order_on_macos() {
        assert_eq!(shortcut_label(&["mod", "shift", "d"], true), "⇧⌘D");
        assert_eq!(shortcut_label(&["mod", "enter"], true), "⌘↵");
        assert_eq!(shortcut_label(&["esc"], true), "Esc");
    }

    #[test]
    fn uses_words_joined_by_plus_on_other_platforms() {
        assert_eq!(
            shortcut_label(&["mod", "shift", "d"], false),
            "Ctrl+Shift+D"
        );
        assert_eq!(shortcut_label(&["mod", "enter"], false), "Ctrl+Enter");
        assert_eq!(shortcut_label(&["mod", "w"], false), "Ctrl+W");
    }

    #[test]
    fn labels_control_separately_from_the_platform_modifier() {
        assert_eq!(shortcut_label(&["ctrl", "shift", "tab"], true), "⌃⇧Tab");
        assert_eq!(shortcut_label(&["ctrl", "tab"], false), "Ctrl+Tab");
    }
}
