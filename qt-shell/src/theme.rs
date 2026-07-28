//! ThemeController — reads/writes `~/.config/opengg/theme.json` via
//! `opengg_core::settings::{load_theme,save_theme}` and derives the palette
//! `Theme.qml` exposes. Mirrors `utils/theme.ts` (`applyTheme`/`applyThemeMode`)
//! and the dark/light palettes hardcoded in `App.vue`'s `:root`/`html.light`.

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
        #[qproperty(QString, accent)]
        #[qproperty(QString, bg)]
        #[qproperty(QString, surface)]
        #[qproperty(QString, border)]
        #[qproperty(QString, text)]
        #[qproperty(QString, text_dim, cxx_name = "textDim")]
        #[qproperty(i32, radius)]
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
    border: QString,
    text: QString,
    text_dim: QString,
    radius: i32,
    dark_mode: bool,
}

impl Default for ThemeControllerRust {
    fn default() -> Self {
        let mut r = Self {
            accent: QString::default(),
            bg: QString::default(),
            surface: QString::default(),
            border: QString::default(),
            text: QString::default(),
            text_dim: QString::default(),
            radius: 6,
            dark_mode: true,
        };
        apply_palette(&mut r, "#E94560", true, 6);
        r
    }
}

const DARK: [&str; 4] = ["#0f1117", "#171923", "#2a2d3a", "#e2e8f0"];
const DARK_TEXT_SEC: &str = "#94a3b8";
const LIGHT: [&str; 4] = ["#f0f2f5", "#ffffff", "#d1d5db", "#111827"];
const LIGHT_TEXT_SEC: &str = "#4b5563";

fn apply_palette(r: &mut ThemeControllerRust, accent: &str, dark: bool, radius: i32) {
    let (palette, text_sec) = if dark {
        (DARK, DARK_TEXT_SEC)
    } else {
        (LIGHT, LIGHT_TEXT_SEC)
    };
    r.accent = QString::from(accent);
    r.bg = QString::from(palette[0]);
    r.surface = QString::from(palette[1]);
    r.border = QString::from(palette[2]);
    r.text = QString::from(palette[3]);
    r.text_dim = QString::from(text_sec);
    r.radius = radius;
    r.dark_mode = dark;
}

/// Parse a CSS length like "6px" → 6; falls back to `default` on any mismatch.
fn parse_px(v: Option<&str>, default: i32) -> i32 {
    v.and_then(|s| s.trim().trim_end_matches("px").parse::<i32>().ok())
        .unwrap_or(default)
}

impl qobject::ThemeController {
    pub fn reload(mut self: Pin<&mut Self>) {
        let raw = opengg_core::settings::load_theme().unwrap_or_default();
        let v: Value = serde_json::from_str(&raw).unwrap_or_default();

        let accent = v["colors"]["--accent"].as_str().unwrap_or("#E94560");
        let dark = v["mode"].as_str().unwrap_or("dark") != "light";
        let radius = parse_px(v["layout"]["--radius"].as_str(), 6);

        let mut computed = ThemeControllerRust {
            accent: QString::default(),
            bg: QString::default(),
            surface: QString::default(),
            border: QString::default(),
            text: QString::default(),
            text_dim: QString::default(),
            radius: 6,
            dark_mode: true,
        };
        apply_palette(&mut computed, accent, dark, radius);

        self.as_mut().set_accent(computed.accent);
        self.as_mut().set_bg(computed.bg);
        self.as_mut().set_surface(computed.surface);
        self.as_mut().set_border(computed.border);
        self.as_mut().set_text(computed.text);
        self.as_mut().set_text_dim(computed.text_dim);
        self.as_mut().set_radius(computed.radius);
        self.as_mut().set_dark_mode(computed.dark_mode);
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
