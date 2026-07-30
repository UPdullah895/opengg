//! ThemeController — reads/writes `~/.config/opengg/theme.json` via
//! `opengg_core::settings::{load_theme,save_theme}` and derives the full design
//! token set `Theme.qml` exposes. Mirrors `utils/theme.ts`
//! (`applyTheme`/`applyThemeMode`) and the dark/light palettes in `App.vue`'s
//! `:root` / `html.light` blocks.
//!
//! Token parity matters concretely: an earlier version of this file exposed
//! only 7 of the ~18 tokens the Vue UI consumes, and every QML panel author
//! then reached for a plausible hex literal instead of a token — producing 84
//! hardcoded colors, 24 of them semantically *wrong* (`#ef4444` where this
//! project's `--danger` is `#dc2626`). Adding a token here is what makes the
//! correct value reachable, so this is the root-cause fix for that class of
//! defect. See `docs/superpowers/plans/2026-07-29-qt6-ui-fidelity-remediation.md`.
//!
//! `applyTheme()` in theme.ts sets *every* key found under `colors`/`layout` as
//! a CSS custom property, so a user's theme.json can override any token — not
//! just `--accent`. `resolve()` below reproduces that generically, keyed by the
//! same CSS property names so theme.json files are portable between both UIs.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // ── Colors (CSS name in the comment) ──
        #[qproperty(QString, accent)]
        #[qproperty(QString, bg)]                                  // --bg-surface
        #[qproperty(QString, surface)]                             // --bg-card
        #[qproperty(QString, bg_deep, cxx_name = "bgDeep")]        // --bg-deep
        #[qproperty(QString, bg_hover, cxx_name = "bgHover")]      // --bg-hover
        #[qproperty(QString, bg_input, cxx_name = "bgInput")]      // --bg-input
        #[qproperty(QString, border)]
        #[qproperty(QString, text)]
        #[qproperty(QString, text_dim, cxx_name = "textDim")]      // --text-sec
        #[qproperty(QString, text_muted, cxx_name = "textMuted")]  // --text-muted
        #[qproperty(QString, danger)]
        #[qproperty(QString, success)]
        #[qproperty(QString, purple)]
        #[qproperty(QString, overdrive)]
        #[qproperty(QString, trimmed)]
        // ── Layout ──
        #[qproperty(i32, radius)]
        #[qproperty(i32, radius_lg, cxx_name = "radiusLg")]
        #[qproperty(i32, clips_grid_cols, cxx_name = "clipsGridCols")]
        #[qproperty(i32, titlebar_h, cxx_name = "titlebarH")]
        #[qproperty(i32, sidebar_w, cxx_name = "sidebarW")]
        #[qproperty(bool, dark_mode, cxx_name = "darkMode")]
        type ThemeController = super::ThemeControllerRust;

        /// Reload from disk (applies on startup and after external changes).
        #[qinvokable]
        fn reload(self: Pin<&mut Self>);

        /// Persist a new accent color + dark/light mode, then re-derive.
        #[qinvokable]
        fn save(self: Pin<&mut Self>, accent_hex: QString, dark: bool);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;
use serde_json::{json, Value};

pub struct ThemeControllerRust {
    accent: QString,
    bg: QString,
    surface: QString,
    bg_deep: QString,
    bg_hover: QString,
    bg_input: QString,
    border: QString,
    text: QString,
    text_dim: QString,
    text_muted: QString,
    danger: QString,
    success: QString,
    purple: QString,
    overdrive: QString,
    trimmed: QString,
    radius: i32,
    radius_lg: i32,
    clips_grid_cols: i32,
    titlebar_h: i32,
    sidebar_w: i32,
    dark_mode: bool,
}

/// The mode-dependent half of the palette — exactly the keys `html.light`
/// overrides in `App.vue`. Everything else (accent/danger/success/purple and
/// all layout values) is mode-independent there, so it lives in the consts
/// below rather than being duplicated per mode.
struct Palette {
    bg: &'static str,
    surface: &'static str,
    bg_deep: &'static str,
    bg_hover: &'static str,
    bg_input: &'static str,
    border: &'static str,
    text: &'static str,
    text_dim: &'static str,
    text_muted: &'static str,
}

/// `App.vue` `:root`
const DARK: Palette = Palette {
    bg: "#0f1117",
    surface: "#171923",
    bg_deep: "#0d0f14",
    bg_hover: "#1e2030",
    bg_input: "#131520",
    border: "#2a2d3a",
    text: "#e2e8f0",
    text_dim: "#94a3b8",
    text_muted: "#4a5568",
};

/// `App.vue` `html.light`
const LIGHT: Palette = Palette {
    bg: "#f0f2f5",
    surface: "#ffffff",
    bg_deep: "#e4e7ec",
    bg_hover: "#dde1e8",
    bg_input: "#f8f9fb",
    border: "#d1d5db",
    text: "#111827",
    text_dim: "#4b5563",
    text_muted: "#9ca3af",
};

const ACCENT: &str = "#E94560";
const DANGER: &str = "#dc2626";
const SUCCESS: &str = "#10b981";
const PURPLE: &str = "#a855f7";
/// Fader tint past 100%. Hardcoded (not a CSS var) in the Vue UI at
/// `ChannelStrip.vue:253` — tokenised here so the shell has no bare literal.
const OVERDRIVE: &str = "#f59e0b";
/// Trimmed-clip duration badge. Hardcoded at `ClipCard.vue:135`.
const TRIMMED: &str = "#ffd27a";

const RADIUS: i32 = 6;
const RADIUS_LG: i32 = 10;
const CLIPS_GRID_COLS: i32 = 4;
const TITLEBAR_H: i32 = 40;
const SIDEBAR_W: i32 = 200;

/// Every token, fully resolved: palette defaults for the active mode with any
/// theme.json override applied on top. Kept as a plain owned struct (no
/// `QString`, no `Pin`) so it is directly unit-testable.
#[derive(Debug, PartialEq)]
pub struct Resolved {
    pub accent: String,
    pub bg: String,
    pub surface: String,
    pub bg_deep: String,
    pub bg_hover: String,
    pub bg_input: String,
    pub border: String,
    pub text: String,
    pub text_dim: String,
    pub text_muted: String,
    pub danger: String,
    pub success: String,
    pub purple: String,
    pub overdrive: String,
    pub trimmed: String,
    pub radius: i32,
    pub radius_lg: i32,
    pub clips_grid_cols: i32,
    pub titlebar_h: i32,
    pub sidebar_w: i32,
    pub dark_mode: bool,
}

/// `theme.json`'s `colors[key]` if it is a non-empty string, else `default`.
fn color(colors: &Value, key: &str, default: &str) -> String {
    colors[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

/// Parse a CSS length like `"6px"` → `6`. Also accepts a bare number (used by
/// `--clips-grid-cols`, which has no unit). Falls back to `default` on any
/// mismatch, and rejects non-positive values — a `0` here would render an
/// invisible sidebar or a division-by-zero grid.
fn parse_len(v: Option<&str>, default: i32) -> i32 {
    v.and_then(|s| s.trim().trim_end_matches("px").trim().parse::<i32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

/// Resolve the full token set from a parsed theme.json value. Accepts any
/// `Value` (including `Null` for "no theme file"), so this never fails.
pub fn resolve(v: &Value) -> Resolved {
    let dark = v["mode"].as_str().unwrap_or("dark") != "light";
    let p = if dark { &DARK } else { &LIGHT };
    let c = &v["colors"];
    let l = &v["layout"];

    Resolved {
        accent: color(c, "--accent", ACCENT),
        bg: color(c, "--bg-surface", p.bg),
        surface: color(c, "--bg-card", p.surface),
        bg_deep: color(c, "--bg-deep", p.bg_deep),
        bg_hover: color(c, "--bg-hover", p.bg_hover),
        bg_input: color(c, "--bg-input", p.bg_input),
        border: color(c, "--border", p.border),
        text: color(c, "--text", p.text),
        text_dim: color(c, "--text-sec", p.text_dim),
        text_muted: color(c, "--text-muted", p.text_muted),
        danger: color(c, "--danger", DANGER),
        success: color(c, "--success", SUCCESS),
        purple: color(c, "--purple", PURPLE),
        overdrive: color(c, "--overdrive", OVERDRIVE),
        trimmed: color(c, "--trimmed", TRIMMED),
        radius: parse_len(l["--radius"].as_str(), RADIUS),
        radius_lg: parse_len(l["--radius-lg"].as_str(), RADIUS_LG),
        clips_grid_cols: parse_len(l["--clips-grid-cols"].as_str(), CLIPS_GRID_COLS),
        titlebar_h: parse_len(l["--titlebar-h"].as_str(), TITLEBAR_H),
        sidebar_w: parse_len(l["--sidebar-w"].as_str(), SIDEBAR_W),
        dark_mode: dark,
    }
}

impl Default for ThemeControllerRust {
    fn default() -> Self {
        let mut r = Self {
            accent: QString::default(),
            bg: QString::default(),
            surface: QString::default(),
            bg_deep: QString::default(),
            bg_hover: QString::default(),
            bg_input: QString::default(),
            border: QString::default(),
            text: QString::default(),
            text_dim: QString::default(),
            text_muted: QString::default(),
            danger: QString::default(),
            success: QString::default(),
            purple: QString::default(),
            overdrive: QString::default(),
            trimmed: QString::default(),
            radius: RADIUS,
            radius_lg: RADIUS_LG,
            clips_grid_cols: CLIPS_GRID_COLS,
            titlebar_h: TITLEBAR_H,
            sidebar_w: SIDEBAR_W,
            dark_mode: true,
        };
        r.apply(resolve(&Value::Null));
        r
    }
}

impl ThemeControllerRust {
    fn apply(&mut self, r: Resolved) {
        self.accent = QString::from(&r.accent);
        self.bg = QString::from(&r.bg);
        self.surface = QString::from(&r.surface);
        self.bg_deep = QString::from(&r.bg_deep);
        self.bg_hover = QString::from(&r.bg_hover);
        self.bg_input = QString::from(&r.bg_input);
        self.border = QString::from(&r.border);
        self.text = QString::from(&r.text);
        self.text_dim = QString::from(&r.text_dim);
        self.text_muted = QString::from(&r.text_muted);
        self.danger = QString::from(&r.danger);
        self.success = QString::from(&r.success);
        self.purple = QString::from(&r.purple);
        self.overdrive = QString::from(&r.overdrive);
        self.trimmed = QString::from(&r.trimmed);
        self.radius = r.radius;
        self.radius_lg = r.radius_lg;
        self.clips_grid_cols = r.clips_grid_cols;
        self.titlebar_h = r.titlebar_h;
        self.sidebar_w = r.sidebar_w;
        self.dark_mode = r.dark_mode;
    }
}

impl qobject::ThemeController {
    pub fn reload(mut self: Pin<&mut Self>) {
        let raw = opengg_core::settings::load_theme().unwrap_or_default();
        let v: Value = serde_json::from_str(&raw).unwrap_or_default();
        let r = resolve(&v);

        self.as_mut().set_accent(QString::from(&r.accent));
        self.as_mut().set_bg(QString::from(&r.bg));
        self.as_mut().set_surface(QString::from(&r.surface));
        self.as_mut().set_bg_deep(QString::from(&r.bg_deep));
        self.as_mut().set_bg_hover(QString::from(&r.bg_hover));
        self.as_mut().set_bg_input(QString::from(&r.bg_input));
        self.as_mut().set_border(QString::from(&r.border));
        self.as_mut().set_text(QString::from(&r.text));
        self.as_mut().set_text_dim(QString::from(&r.text_dim));
        self.as_mut().set_text_muted(QString::from(&r.text_muted));
        self.as_mut().set_danger(QString::from(&r.danger));
        self.as_mut().set_success(QString::from(&r.success));
        self.as_mut().set_purple(QString::from(&r.purple));
        self.as_mut().set_overdrive(QString::from(&r.overdrive));
        self.as_mut().set_trimmed(QString::from(&r.trimmed));
        self.as_mut().set_radius(r.radius);
        self.as_mut().set_radius_lg(r.radius_lg);
        self.as_mut().set_clips_grid_cols(r.clips_grid_cols);
        self.as_mut().set_titlebar_h(r.titlebar_h);
        self.as_mut().set_sidebar_w(r.sidebar_w);
        self.as_mut().set_dark_mode(r.dark_mode);
    }

    pub fn save(self: Pin<&mut Self>, accent_hex: QString, dark: bool) {
        let raw = opengg_core::settings::load_theme().unwrap_or_default();
        let mut v: Value = serde_json::from_str(&raw).unwrap_or_default();
        if !v.is_object() {
            v = json!({});
        }
        if !v["colors"].is_object() {
            v["colors"] = json!({});
        }
        v["colors"]["--accent"] = json!(accent_hex.to_string());
        v["mode"] = json!(if dark { "dark" } else { "light" });

        if let Ok(s) = serde_json::to_string_pretty(&v) {
            let _ = opengg_core::settings::save_theme(&s);
        }
        self.reload();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards token parity with `App.vue`'s `:root`. If someone edits the CSS
    /// without editing this file (or vice versa), this fails loudly rather than
    /// letting the two UIs drift apart again.
    #[test]
    fn dark_defaults_match_the_css_root_block() {
        let r = resolve(&Value::Null);
        assert_eq!(r.bg, "#0f1117", "--bg-surface");
        assert_eq!(r.surface, "#171923", "--bg-card");
        assert_eq!(r.bg_deep, "#0d0f14", "--bg-deep");
        assert_eq!(r.bg_hover, "#1e2030", "--bg-hover");
        assert_eq!(r.bg_input, "#131520", "--bg-input");
        assert_eq!(r.border, "#2a2d3a", "--border");
        assert_eq!(r.text, "#e2e8f0", "--text");
        assert_eq!(r.text_dim, "#94a3b8", "--text-sec");
        assert_eq!(r.text_muted, "#4a5568", "--text-muted");
        assert_eq!(r.accent, "#E94560", "--accent");
        assert_eq!(r.danger, "#dc2626", "--danger");
        assert_eq!(r.success, "#10b981", "--success");
        assert_eq!(r.purple, "#a855f7", "--purple");
        assert_eq!(r.overdrive, "#f59e0b", "overdrive (ChannelStrip.vue:253)");
        assert_eq!(r.radius, 6, "--radius");
        assert_eq!(r.radius_lg, 10, "--radius-lg");
        assert_eq!(r.clips_grid_cols, 4, "--clips-grid-cols");
        assert_eq!(r.titlebar_h, 40, "--titlebar-h");
        assert_eq!(r.sidebar_w, 200, "--sidebar-w");
        assert!(r.dark_mode);
    }

    /// Guards token parity with `App.vue`'s `html.light`. Only the nine
    /// background/border/text keys differ; the rest must stay mode-independent.
    #[test]
    fn light_defaults_match_the_css_light_block() {
        let r = resolve(&json!({ "mode": "light" }));
        assert_eq!(r.bg, "#f0f2f5");
        assert_eq!(r.surface, "#ffffff");
        assert_eq!(r.bg_deep, "#e4e7ec");
        assert_eq!(r.bg_hover, "#dde1e8");
        assert_eq!(r.bg_input, "#f8f9fb");
        assert_eq!(r.border, "#d1d5db");
        assert_eq!(r.text, "#111827");
        assert_eq!(r.text_dim, "#4b5563");
        assert_eq!(r.text_muted, "#9ca3af");
        assert!(!r.dark_mode);
        // Mode-independent in the CSS — light mode must not change these.
        assert_eq!(r.accent, "#E94560");
        assert_eq!(r.danger, "#dc2626");
        assert_eq!(r.success, "#10b981");
        assert_eq!(r.purple, "#a855f7");
        assert_eq!(r.overdrive, "#f59e0b");
        assert_eq!(r.radius, 6);
    }

    /// theme.ts's `applyTheme` sets every key it finds, so *any* token must be
    /// overridable — not just `--accent`.
    #[test]
    fn any_color_token_can_be_overridden() {
        let r = resolve(&json!({
            "colors": { "--bg-card": "#123456", "--success": "#00ff00" }
        }));
        assert_eq!(r.surface, "#123456");
        assert_eq!(r.success, "#00ff00");
        // Untouched keys keep their palette default.
        assert_eq!(r.bg, "#0f1117");
    }

    #[test]
    fn layout_overrides_parse_px_and_bare_numbers() {
        let r = resolve(&json!({
            "layout": { "--radius": "12px", "--clips-grid-cols": "6", "--sidebar-w": "240px" }
        }));
        assert_eq!(r.radius, 12);
        assert_eq!(r.clips_grid_cols, 6);
        assert_eq!(r.sidebar_w, 240);
        assert_eq!(r.radius_lg, 10, "unset layout keys keep their default");
    }

    /// A zero or negative value would render an invisible sidebar or divide by
    /// zero in the clips grid, so it must fall back rather than be honoured.
    #[test]
    fn nonpositive_and_garbage_lengths_fall_back() {
        let r = resolve(&json!({
            "layout": { "--clips-grid-cols": "0", "--sidebar-w": "-40px", "--radius": "wide" }
        }));
        assert_eq!(r.clips_grid_cols, 4);
        assert_eq!(r.sidebar_w, 200);
        assert_eq!(r.radius, 6);
    }

    /// An empty string in theme.json is a common hand-editing artifact and must
    /// not blank out a color.
    #[test]
    fn empty_color_override_falls_back() {
        let r = resolve(&json!({ "colors": { "--accent": "  " } }));
        assert_eq!(r.accent, "#E94560");
    }

    /// The real shipped theme.json only sets --accent and --clips-grid-cols;
    /// it must resolve to the dark palette with those two applied.
    #[test]
    fn shipped_theme_shape_resolves() {
        let r = resolve(&json!({
            "colors": { "--accent": "#e94560" },
            "layout": { "--clips-grid-cols": "4" },
            "mode": "dark"
        }));
        assert_eq!(r.accent, "#e94560");
        assert_eq!(r.clips_grid_cols, 4);
        assert_eq!(r.surface, "#171923");
        assert!(r.dark_mode);
    }
}
