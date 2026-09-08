//! GTK-compatible terminal colors and typography, resolved without a GTK dependency.
use std::{
    collections::HashMap,
    env, fs,
    io::Read,
    os::unix::fs::MetadataExt,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Theme {
    colors: HashMap<&'static str, String>,
    family: String,
    size: f64,
    selection_bg: String,
    selection_fg: String,
    cursor: String,
    terminal: bool,
    sources: Vec<PathBuf>,
}

const KEYS: [&str; 12] = [
    "background",
    "dark_background",
    "darker_background",
    "lighter_background",
    "foreground",
    "dark_foreground",
    "muted",
    "accent",
    "red",
    "yellow",
    "green",
    "blue",
];
const FALLBACK: [&str; 12] = [
    "#3a332a", "#2e2820", "#241f19", "#463e33", "#f5ecd9", "#b0a48f", "#968872", "#d6a06b",
    "#ed806b", "#dab46b", "#83baa1", "#74accf",
];

fn valid_color(s: &str) -> bool {
    let b = s.as_bytes();
    matches!(b.len(), 4 | 7 | 9) && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)
}
fn parse_toml_colors(text: &str) -> Option<HashMap<String, String>> {
    Some(
        text.parse::<toml::Value>()
            .ok()?
            .as_table()?
            .iter()
            .filter_map(|(k, v)| {
                let value = v.as_str()?.trim();
                valid_color(value).then(|| (k.clone(), value.to_string()))
            })
            .collect(),
    )
}
fn parse_settings(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (k, v) = line.split_once('=')?;
            Some((k.trim().into(), v.trim().into()))
        })
        .collect()
}
fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_else(|| "/".into()))
}
fn command(args: &[&str]) -> Option<String> {
    let mut child = Command::new(args[0])
        .args(&args[1..])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = sender.send(stdout.read_to_end(&mut bytes).ok().map(|_| bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGKILL);
                }
                let _ = child.wait();
                return None;
            }
            Err(_) => {
                unsafe {
                    libc::killpg(child.id() as i32, libc::SIGKILL);
                }
                let _ = child.wait();
                return None;
            }
        }
    };
    let bytes = match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(bytes) => bytes?,
        Err(_) => {
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
            return None;
        }
    };
    if !status.success() {
        return None;
    }
    let value = String::from_utf8(bytes).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.into())
}
fn rgb(c: &str) -> (f64, f64, f64) {
    let x = c.trim_start_matches('#');
    let x = if x.len() == 3 {
        format!(
            "{}{}{}{}{}{}",
            &x[0..1],
            &x[0..1],
            &x[1..2],
            &x[1..2],
            &x[2..3],
            &x[2..3]
        )
    } else {
        x[..6].to_string()
    };
    (
        u8::from_str_radix(&x[0..2], 16).unwrap_or(0) as f64,
        u8::from_str_radix(&x[2..4], 16).unwrap_or(0) as f64,
        u8::from_str_radix(&x[4..6], 16).unwrap_or(0) as f64,
    )
}
fn mix(a: &str, b: &str, n: f64) -> String {
    let (ar, ag, ab) = rgb(a);
    let (br, bg, bb) = rgb(b);
    format!(
        "#{:02x}{:02x}{:02x}",
        (ar + (br - ar) * n).round() as u8,
        (ag + (bg - ag) * n).round() as u8,
        (ab + (bb - ab) * n).round() as u8
    )
}
// Qt uses #AARRGGBB, while the source palettes use CSS #RRGGBBAA.
fn qt_color(c: &str) -> String {
    if c.len() == 9 {
        format!("#{}{}", &c[7..9], &c[1..7])
    } else {
        c.into()
    }
}
fn alpha(c: &str, a: f64) -> String {
    let (r, g, b) = rgb(c);
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        (a * 255.).round() as u8,
        r as u8,
        g as u8,
        b as u8
    )
}
fn luminance(c: &str) -> f64 {
    let (r, g, b) = rgb(c);
    let f = |x: f64| {
        let x = x / 255.;
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
}
fn contrast(a: &str, b: &str) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    let (l, d) = if a > b { (a, b) } else { (b, a) };
    (l + 0.05) / (d + 0.05)
}
fn readable(candidate: &str, foreground: &str, surfaces: &[&str]) -> String {
    if surfaces.iter().all(|s| contrast(candidate, s) >= 4.5) {
        return candidate.into();
    }
    for n in 1..=20 {
        let c = mix(candidate, foreground, n as f64 / 20.);
        if surfaces.iter().all(|s| contrast(&c, s) >= 4.5) {
            return c;
        }
    }
    foreground.into()
}
fn fixed_sources(h: &Path) -> Vec<PathBuf> {
    let state = h.join(".local/state/omarchy/current");
    let legacy = h.join(".config/omarchy/current");
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| h.join(".config"));
    vec![
        state.clone(),
        state.join("theme"),
        state.join("theme.name"),
        state.join("theme/colors.toml"),
        state.join("theme/shell.toml"),
        legacy.clone(),
        legacy.join("theme"),
        legacy.join("theme.name"),
        legacy.join("theme/colors.toml"),
        legacy.join("theme/shell.toml"),
        h.join(".config/omarchy/shell.toml"),
        h.join(".config/fontconfig/fonts.conf"),
        config.join("ghostty/config"),
        config.join("ghostty/config.ghostty"),
        state.join("theme/ghostty.conf"),
        legacy.join("theme/ghostty.conf"),
    ]
}

fn derive(data: &HashMap<String, String>) -> HashMap<&'static str, String> {
    let bg = &data["background"];
    let fg = &data["foreground"];
    let edge = if luminance(bg) < luminance(fg) {
        "#000000"
    } else {
        "#ffffff"
    };
    let mut p = HashMap::from([
        ("background", bg.clone()),
        ("foreground", fg.clone()),
        ("dark_background", mix(bg, edge, 0.16)),
        ("darker_background", mix(bg, edge, 0.28)),
        ("lighter_background", mix(bg, fg, 0.10)),
    ]);
    for (role, aliases) in [
        (
            "dark_foreground",
            ["light_foreground", "muted", "foreground", "foreground"],
        ),
        (
            "muted",
            [
                "light_foreground",
                "dark_foreground",
                "foreground",
                "foreground",
            ],
        ),
        ("accent", ["blue", "cyan", "green", "foreground"]),
        ("red", ["bright_red", "accent", "foreground", "foreground"]),
        (
            "yellow",
            ["bright_yellow", "orange", "accent", "foreground"],
        ),
        ("green", ["bright_green", "cyan", "accent", "foreground"]),
        ("blue", ["bright_blue", "cyan", "accent", "foreground"]),
    ] {
        let value = data
            .get(role)
            .or_else(|| aliases.iter().find_map(|k| data.get(*k)))
            .unwrap_or(fg)
            .clone();
        p.insert(role, value);
    }
    for key in KEYS {
        if let Some(v) = data.get(key) {
            p.insert(key, v.clone());
        }
    }
    p
}

impl Theme {
    pub fn resolve() -> Self {
        let mut colors: HashMap<_, _> = KEYS
            .into_iter()
            .zip(FALLBACK.into_iter().map(str::to_string))
            .collect();
        let h = home();
        let mut sources = fixed_sources(&h);
        let candidates = [
            h.join(".local/state/omarchy/current/theme/colors.toml"),
            h.join(".config/omarchy/current/theme/colors.toml"),
        ];
        let mut palette = candidates.into_iter().find(|p| p.is_file());
        if palette.is_none() {
            let name = [
                h.join(".local/state/omarchy/current/theme.name"),
                h.join(".config/omarchy/current/theme.name"),
            ]
            .into_iter()
            .find_map(|p| fs::read_to_string(p).ok().filter(|s| !s.trim().is_empty()))
            .or_else(|| command(&["omarchy", "theme", "current"]));
            if let Some(n) = name {
                let slug = n.trim().to_lowercase().replace(' ', "-");
                palette = [
                    h.join(format!(".config/omarchy/themes/{slug}/colors.toml")),
                    PathBuf::from(format!("/usr/share/omarchy/themes/{slug}/colors.toml")),
                ]
                .into_iter()
                .find(|p| p.is_file())
                .or_else(|| {
                    command(&["omarchy", "theme", "dir", &slug])
                        .map(PathBuf::from)
                        .map(|p| p.join("colors.toml"))
                        .filter(|p| p.is_file())
                });
            }
        }
        if let Some(p) = palette {
            if !sources.contains(&p) {
                sources.push(p.clone())
            }
            if let Ok(t) = fs::read_to_string(&p)
                && let Some(d) = parse_toml_colors(&t)
                && d.get("background").is_some_and(|v| valid_color(v))
                && d.get("foreground").is_some_and(|v| valid_color(v))
            {
                colors = if KEYS.iter().all(|k| d.contains_key(*k)) {
                    KEYS.into_iter().map(|k| (k, d[k].clone())).collect()
                } else {
                    derive(&d)
                };
            }
        }
        let mut family = None;
        let mut size = 11.;
        let shell = h.join(".config/omarchy/shell.toml");
        if let Ok(t) = fs::read_to_string(shell)
            && let Ok(v) = t.parse::<toml::Value>()
            && let Some(n) = v
                .get("font")
                .and_then(|v| v.get("base-size"))
                .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
                .filter(|n| (6.0..=40.0).contains(n))
        {
            size = n
        }
        let mut selection_bg = format!("{}73", mix(&colors["accent"], &colors["accent"], 0.));
        let mut selection_fg = colors["foreground"].clone();
        let mut cursor = colors["accent"].clone();
        let mut terminal = false;
        if let Some(cfg) = command(&["ghostty", "+show-config"]) {
            let d = parse_settings(&cfg);
            for line in cfg.lines() {
                if let Some(path) = line
                    .split_once('=')
                    .filter(|(k, _)| k.trim() == "config-file")
                    .map(|(_, v)| v.trim().strip_prefix('?').unwrap_or(v.trim()))
                {
                    let p = if let Some(rest) = path.strip_prefix("~/") {
                        h.join(rest)
                    } else {
                        PathBuf::from(path)
                    };
                    if p.is_absolute() && !sources.contains(&p) {
                        sources.push(p)
                    }
                }
            }
            if d.get("background").is_some_and(|v| valid_color(v))
                && d.get("foreground").is_some_and(|v| valid_color(v))
            {
                terminal = true;
                family = d.get("font-family").filter(|v| !v.is_empty()).cloned();
                let bg = d["background"].clone();
                let fg = d["foreground"].clone();
                for k in [
                    "background",
                    "dark_background",
                    "darker_background",
                    "lighter_background",
                ] {
                    colors.insert(k, bg.clone());
                }
                colors.insert("foreground", fg.clone());
                for line in cfg.lines() {
                    if let Some((idx, color)) = line
                        .split_once('=')
                        .filter(|(k, _)| k.trim() == "palette")
                        .and_then(|(_, v)| v.trim().split_once('='))
                    {
                        let roles = match idx.trim() {
                            "1" => &["red"][..],
                            "2" => &["green"][..],
                            "3" => &["yellow"][..],
                            "4" => &["blue", "accent"][..],
                            "8" => &["muted", "dark_foreground"][..],
                            _ => continue,
                        };
                        if valid_color(color.trim()) {
                            for role in roles {
                                colors.insert(role, color.trim().into());
                            }
                        }
                    }
                }
                if let Some(v) = d
                    .get("font-size")
                    .and_then(|v| v.parse().ok())
                    .filter(|v: &f64| (6.0..=40.0).contains(v))
                {
                    size = v
                }
                selection_bg = d
                    .get("selection-background")
                    .filter(|v| valid_color(v))
                    .cloned()
                    .unwrap_or_else(|| colors["accent"].clone());
                selection_fg = d
                    .get("selection-foreground")
                    .filter(|v| valid_color(v))
                    .cloned()
                    .unwrap_or_else(|| colors["foreground"].clone());
                cursor = d
                    .get("cursor-color")
                    .filter(|v| valid_color(v))
                    .cloned()
                    .unwrap_or_else(|| colors["accent"].clone());
            }
        }
        let family = family.unwrap_or_else(|| {
            command(&["omarchy", "font", "current"])
                .unwrap_or_else(|| "JetBrainsMono Nerd Font".into())
        });
        Self {
            colors,
            family,
            size,
            selection_bg,
            selection_fg,
            cursor,
            terminal,
            sources,
        }
    }
    pub fn fingerprint(&self) -> String {
        self.sources
            .iter()
            .map(|p| {
                fs::metadata(p)
                    .ok()
                    .map(|m| {
                        format!(
                            "{}:{}:{}:{}:{}:{}",
                            p.display(),
                            m.dev(),
                            m.ino(),
                            m.size(),
                            m.mtime(),
                            m.mtime_nsec()
                        )
                    })
                    .unwrap_or_else(|| format!("{}:-", p.display()))
            })
            .collect::<Vec<_>>()
            .join("|")
    }
    pub fn to_json(&self) -> serde_json::Value {
        let c = &self.colors;
        let bg = &c["background"];
        let fg = &c["foreground"];
        let muted = readable(
            &c["muted"],
            fg,
            &[bg, &c["lighter_background"], &c["darker_background"]],
        );
        let small = if self.terminal {
            self.size
        } else {
            (self.size * 0.82).max(self.size.min(8.))
        };
        serde_json::json!({
            "background": qt_color(bg),
            "control": mix(bg, "#000000", 0.05),
            "foreground": qt_color(fg),
            "muted": qt_color(&muted),
            "accent": qt_color(&c["accent"]),
            "red": qt_color(&c["red"]),
            "yellow": qt_color(&c["yellow"]),
            "green": qt_color(&c["green"]),
            "hair": alpha(fg, 0.25),
            "selectionBg": qt_color(&self.selection_bg),
            "selectionFg": qt_color(&self.selection_fg),
            "cursor": qt_color(&self.cursor),
            "fontFamily": self.family.replace(['\r', '\n'], " "),
            "fontPointSize": self.size,
            "smallPointSize": small,
        })
    }
}

/// A backend stays alive between snapshots; only rerun terminal commands after
/// an observed theme/font/config source changes.
#[derive(Default)]
pub struct ThemeCache {
    resolved: Option<(Theme, String, serde_json::Value)>,
}
impl ThemeCache {
    pub fn current(&mut self) -> &serde_json::Value {
        if self
            .resolved
            .as_ref()
            .is_none_or(|(theme, fingerprint, _)| theme.fingerprint() != *fingerprint)
        {
            let theme = Theme::resolve();
            let fingerprint = theme.fingerprint();
            let json = theme.to_json();
            self.resolved = Some((theme, fingerprint, json));
        }
        &self.resolved.as_ref().unwrap().2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_trims_successful_output() {
        assert_eq!(
            command(&["/bin/sh", "-c", "printf ' font \\n'"]),
            Some("font".into())
        );
        assert_eq!(command(&["/bin/sh", "-c", "printf font; exit 1"]), None);
    }
    #[test]
    fn command_times_out_when_grandchild_holds_stdout() {
        let started = Instant::now();
        assert_eq!(command(&["/bin/sh", "-c", "sleep 10 & printf font"]), None);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
    #[test]
    fn sparse_aliases_and_contrast() {
        let d = HashMap::from([
            ("background".into(), "#faf4ed".into()),
            ("foreground".into(), "#575279".into()),
            ("blue".into(), "#286983".into()),
        ]);
        let p = derive(&d);
        assert_eq!(p["accent"], "#286983");
        let c = readable("#cecacd", "#575279", &["#faf4ed", "#f2e9e1", "#fffaf3"]);
        assert!(
            ["#faf4ed", "#f2e9e1", "#fffaf3"]
                .iter()
                .all(|s| contrast(&c, s) >= 4.5)
        );
    }
    #[test]
    fn ghostty_parser_preserves_hash_colors() {
        let p = parse_settings("background = #05182e\nforeground = #f6dcac\n");
        assert_eq!(p["background"], "#05182e");
    }

    #[test]
    fn terminal_theme_keeps_eight_point_supporting_text() {
        let theme = Theme {
            colors: KEYS
                .into_iter()
                .zip(FALLBACK.into_iter().map(str::to_string))
                .collect(),
            family: "A \"font\"".into(),
            size: 8.0,
            selection_bg: "#134e5a".into(),
            selection_fg: "#f6dcac".into(),
            cursor: "#faa968".into(),
            terminal: true,
            sources: vec![],
        };
        let value = theme.to_json();
        assert_eq!(value["fontFamily"], "A \"font\"");
        assert_eq!(value["fontPointSize"], 8.0);
        assert_eq!(value["smallPointSize"], 8.0);
        assert_eq!(value["selectionBg"], "#134e5a");
        assert_eq!(value["selectionFg"], "#f6dcac");
        assert_eq!(value["cursor"], "#faa968");
        assert_eq!(value["control"], "#373028");
        assert_eq!(value["hair"], "#40f5ecd9");
        assert_eq!(qt_color("#12345678"), "#78123456");
    }
}
