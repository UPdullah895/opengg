//! ScreenshotController — dev-only headless UI capture (UI-fidelity plan Phase 0).
//!
//! Exists so QML design work can be *seen* without borrowing the developer's
//! display. Every earlier UI feature in this migration was signed off on a
//! clean build + empty log, because the only monitor was occupied and
//! `hyprctl dispatch` is broken in that environment — which is precisely how a
//! large visual-fidelity gap accumulated unnoticed. Rendering offscreen and
//! grabbing a PNG removes that tradeoff entirely: no focus steal, no window
//! popping over a fullscreen game.
//!
//! Usage (see `make ui-shots`):
//!   QT_QPA_PLATFORM=offscreen opengg-qt --screenshot out.png \
//!       [--page clips] [--panel notifications] [--delay 900]
//!
//! Args are parsed from `std::env::args()` inside `Default::default()` rather
//! than being pushed in from `main()`: cxx-qt constructs `qml_singleton`
//! QObjects lazily when QML first resolves them, so there is no reliable
//! pre-QML window in which a setter could run.

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
        /// True when `--screenshot <path>` was passed. QML gates the whole
        /// capture-then-quit path on this, so a normal run is unaffected.
        #[qproperty(bool, active)]
        #[qproperty(QString, output_path, cxx_name = "outputPath")]
        /// Target page id ("home"/"mixer"/"clips"/"devices"/"settings"), or empty.
        #[qproperty(QString, page)]
        /// Settings sub-panel key when `page == "settings"`, or empty.
        #[qproperty(QString, panel)]
        /// Settle delay before grabbing, in ms — covers async thumbnail loads,
        /// D-Bus refreshes and the first Qt Quick frame.
        #[qproperty(i32, delay_ms, cxx_name = "delayMs")]
        /// True for `--with-tour`: capture the guided-tour overlay instead of
        /// suppressing it (it would otherwise cover every other screenshot).
        #[qproperty(bool, with_tour, cxx_name = "withTour")]
        type ScreenshotController = super::ScreenshotControllerRust;

        /// Log the outcome of a capture attempt (QML has no stderr of its own).
        #[qinvokable]
        fn report(self: Pin<&mut Self>, ok: bool, detail: QString);
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

pub struct ScreenshotControllerRust {
    active: bool,
    output_path: QString,
    page: QString,
    panel: QString,
    delay_ms: i32,
    with_tour: bool,
}

/// Value of `--flag <value>`, or None when absent/valueless.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .filter(|v| !v.starts_with("--"))
        .cloned()
}

impl Default for ScreenshotControllerRust {
    fn default() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let output_path = flag_value(&args, "--screenshot");
        Self {
            active: output_path.is_some(),
            output_path: QString::from(output_path.unwrap_or_default().as_str()),
            page: QString::from(flag_value(&args, "--page").unwrap_or_default().as_str()),
            panel: QString::from(flag_value(&args, "--panel").unwrap_or_default().as_str()),
            delay_ms: flag_value(&args, "--delay")
                .and_then(|v| v.parse().ok())
                .unwrap_or(900),
            with_tour: args.iter().any(|a| a == "--with-tour"),
        }
    }
}

impl qobject::ScreenshotController {
    pub fn report(self: Pin<&mut Self>, ok: bool, detail: QString) {
        if ok {
            eprintln!("[screenshot] wrote {detail}");
        } else {
            eprintln!("[screenshot] FAILED: {detail}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::flag_value;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_a_flag_value() {
        assert_eq!(
            flag_value(&v(&["opengg-qt", "--screenshot", "out.png"]), "--screenshot"),
            Some("out.png".into())
        );
    }

    #[test]
    fn missing_flag_is_none() {
        assert_eq!(flag_value(&v(&["opengg-qt"]), "--screenshot"), None);
    }

    /// A flag immediately followed by another flag has no value — without this
    /// guard `--screenshot --page clips` would silently capture to a file
    /// literally named "--page".
    #[test]
    fn flag_followed_by_flag_is_none() {
        assert_eq!(
            flag_value(&v(&["opengg-qt", "--screenshot", "--page"]), "--screenshot"),
            None
        );
    }

    #[test]
    fn trailing_flag_with_no_value_is_none() {
        assert_eq!(
            flag_value(&v(&["opengg-qt", "--page"]), "--page"),
            None
        );
    }
}
