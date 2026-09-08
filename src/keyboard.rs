use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn defaults() -> BTreeMap<String, Vec<String>> {
    [
        ("save_note", vec!["Ctrl+Return", "Ctrl+S"]),
        ("delete_note", vec!["Ctrl+Delete"]),
        ("focus_note", vec!["Ctrl+N"]),
        ("search", vec!["Ctrl+F"]),
        ("projects", vec!["Ctrl+P"]),
        ("new_project", vec!["Ctrl+Shift+N"]),
        ("open_folder", vec!["Ctrl+O"]),
        ("add_section", vec!["Ctrl+Shift+S"]),
        ("clip", vec!["Ctrl+Shift+C"]),
        ("toggle_ai", vec!["Ctrl+Shift+A"]),
        ("cycle_opacity", vec!["Ctrl+Shift+O"]),
        ("minimize", vec!["Ctrl+M"]),
        ("end_session", vec!["Ctrl+Shift+W"]),
        ("previous_entry", vec!["Alt+Up"]),
        ("next_entry", vec!["Alt+Down"]),
        ("preview", vec!["Alt+Return"]),
        ("expand_note", vec!["Shift+Alt+Return"]),
        ("help", vec!["Ctrl+H", "F1"]),
    ]
    .into_iter()
    .map(|(action, keys)| (action.into(), keys.into_iter().map(String::from).collect()))
    .collect()
}

pub(crate) fn config_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| ui_notes::core::home().join(".config"))
        .join("ui-notes/keys.toml")
}

// Accept the existing GTK accelerator spelling as well as Qt's portable spelling.
fn accelerator(text: &str) -> Result<String, String> {
    let mut rest = text;
    let mut parts = Vec::new();
    while let Some(suffix) = rest.strip_prefix('<') {
        let (modifier, suffix) = suffix.split_once('>').ok_or("unclosed modifier")?;
        parts.push(modifier);
        rest = suffix;
    }
    parts.extend(rest.split('+'));
    let key = parts.pop().ok_or("empty accelerator")?;
    let mut modifiers = BTreeSet::new();
    for part in parts {
        let modifier = match part.to_ascii_lowercase().as_str() {
            "control" | "ctrl" | "primary" => "Ctrl",
            "shift" => "Shift",
            "alt" | "mod1" => "Alt",
            "super" | "meta" => "Meta",
            _ => return Err(format!("unsupported modifier {part}")),
        };
        if !modifiers.insert(modifier) {
            return Err("duplicate modifier".into());
        }
    }
    let key = match key.to_ascii_lowercase().as_str() {
        "return" | "enter" | "kp_enter" => "Return".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "page_up" | "pageup" | "pgup" => "PgUp".into(),
        "page_down" | "pagedown" | "pgdown" => "PgDown".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "space" => "Space".into(),
        "escape" | "tab" | "iso_left_tab" => {
            return Err("Escape and Tab are reserved for navigation".into());
        }
        lower
            if lower.starts_with('f')
                && lower[1..]
                    .parse::<u8>()
                    .is_ok_and(|n| (1..=35).contains(&n)) =>
        {
            lower.to_ascii_uppercase()
        }
        _ if key.len() == 1
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ",.;/[]'=-`".contains(c)) =>
        {
            key.to_ascii_uppercase()
        }
        _ => return Err(format!("unsupported key {key}")),
    };
    let function = key.starts_with('F') && key[1..].parse::<u8>().is_ok();
    if (modifiers.is_empty() && !function)
        || (modifiers.len() == 1 && modifiers.contains("Shift") && key != "Return")
    {
        return Err(
            "bare keys are reserved for typing; add a modifier or use a function key".into(),
        );
    }
    let mut out = Vec::new();
    for modifier in ["Ctrl", "Shift", "Alt", "Meta"] {
        if modifiers.contains(modifier) {
            out.push(modifier.to_owned());
        }
    }
    out.push(key);
    Ok(out.join("+"))
}

fn parse(text: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let root: toml::Table = toml::from_str(text).map_err(|e| format!("malformed TOML: {e}"))?;
    for name in root.keys() {
        if name != "keys" {
            return Err(format!("unknown setting {name}"));
        }
    }
    let mut bindings = defaults();
    if let Some(keys) = root.get("keys") {
        for (action, value) in keys.as_table().ok_or("keys must be a table")? {
            if !bindings.contains_key(action) {
                return Err(format!("unknown keyboard action {action}"));
            }
            let values = value
                .as_array()
                .ok_or("bindings must be arrays")?
                .iter()
                .map(|value| accelerator(value.as_str().ok_or("bindings must contain strings")?))
                .collect::<Result<Vec<_>, String>>()?;
            bindings.insert(action.clone(), values);
        }
    }
    let mut used = BTreeMap::new();
    for (action, keys) in &bindings {
        for key in keys {
            if let Some(other) = used.insert(key, action) {
                return Err(format!("duplicate binding {key} for {other} and {action}"));
            }
        }
    }
    Ok(bindings)
}

pub fn load() -> (Value, String) {
    let path = config_path();
    let result = match std::fs::read_to_string(&path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(defaults()),
        Err(error) => Err(error.to_string()),
    };
    match result {
        Ok(keys) => (json!(keys), String::new()),
        Err(error) => (
            json!(defaults()),
            format!(
                "Invalid keyboard config {}: {error}. Using defaults.",
                path.display()
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_overrides_and_disabled_actions() {
        let keys = parse("[keys]\nsearch = ['<Control>g']\nprojects = []\n").unwrap();
        assert_eq!(keys["search"], ["Ctrl+G"]);
        assert!(keys["projects"].is_empty());
        assert_eq!(keys["focus_note"], ["Ctrl+N"]);
        assert_eq!(keys["save_note"], ["Ctrl+Return", "Ctrl+S"]);
    }
    #[test]
    fn conflicts_and_reserved_keys_are_rejected() {
        assert!(
            parse("[keys]\nsearch = ['<Control>n']")
                .unwrap_err()
                .contains("duplicate")
        );
        assert!(parse("[keys]\npreview = ['Alt+Return', '<Alt>KP_Enter']").is_err());
        for key in ["x", "Shift+x", "Ctrl+Escape", "<Hyper>h", ""] {
            assert!(accelerator(key).is_err(), "{key}");
        }
        assert!(parse("[keys]\nsearch = ['Ctrl+N']\nfocus_note = ['Ctrl+F']").is_ok());
    }
}
