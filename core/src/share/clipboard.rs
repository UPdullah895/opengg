//! Put exported files on the system clipboard, and the cheap "copy a short
//! part" path that needs no re-encode.
//!
//! Files go on the clipboard as `text/uri-list`: that is what file managers
//! publish, and what Discord / browsers / chat apps accept when the user hits
//! Ctrl+V — the file is attached, not a path pasted as text.

use super::util::{part_path, percent_encode, secs_arg, unique_path, which};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `text/uri-list` body for `paths`: one `file://` URI per line, CRLF line
/// endings (RFC 2483). Everything except unreserved characters and `/` is
/// percent-encoded as UTF-8, so spaces and non-Latin names survive.
/// Relative paths are made absolute against the current directory.
pub fn uri_list<P: AsRef<Path>>(paths: &[P]) -> String {
    let mut out = String::new();
    for p in paths {
        let p = p.as_ref();
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
        out.push_str("file://");
        out.push_str(&percent_encode(&abs.to_string_lossy(), b"/"));
        out.push_str("\r\n");
    }
    out
}

/// Copy files to the clipboard. Uses `wl-copy` on Wayland sessions (when
/// installed), `xclip` otherwise (also works under XWayland).
pub fn copy_files_to_clipboard<P: AsRef<Path>>(paths: &[P]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("Nothing to copy".into());
    }
    let body = uri_list(paths);
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty());
    let mut cmd = if wayland && which("wl-copy") {
        let mut c = Command::new("wl-copy");
        c.args(["--type", "text/uri-list"]);
        c
    } else if which("xclip") {
        let mut c = Command::new("xclip");
        c.args(["-selection", "clipboard", "-t", "text/uri-list"]);
        c
    } else {
        return Err("Install wl-clipboard (Wayland) or xclip (X11) to copy files".into());
    };
    // Both tools fork a background owner that serves the selection; null
    // stdout/stderr so it never holds our pipes open.
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("clipboard: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .map_err(|e| format!("clipboard: {e}"))?;
    }
    let status = child.wait().map_err(|e| format!("clipboard: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("clipboard tool failed ({status})"))
    }
}

/// Tolerance for "the window is the whole clip".
const WHOLE_CLIP_EPS: f64 = 0.05;

/// Stream-copy `[start, end]` of `input` into `out` (no re-encode, so it is
/// instant and lossless; cut points snap to keyframes). All audio tracks are
/// kept. When the window is the whole clip the original path is returned
/// unchanged — there is nothing to cut.
///
/// `out` is never overwritten: a collision becomes "name (2).mp4". The file
/// is written to a hidden part file first and renamed when complete.
pub fn lossless_cut(input: &Path, start: f64, end: f64, out: &Path) -> Result<PathBuf, String> {
    let dur = crate::media::probe_duration(&input.to_string_lossy());
    if dur > 0.0 && start <= WHOLE_CLIP_EPS && end >= dur - WHOLE_CLIP_EPS {
        return Ok(input.to_path_buf());
    }
    if end <= start {
        return Err("Empty selection".into());
    }
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let final_path = unique_path(out)?;
    let part = part_path(&final_path);
    let res = Command::new("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-y", "-ss"])
        .arg(secs_arg(start))
        .arg("-to")
        .arg(secs_arg(end))
        .arg("-i")
        .arg(input)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-avoid_negative_ts",
            "make_zero",
            "-movflags",
            "+faststart",
        ])
        .arg(&part)
        .stdout(Stdio::null())
        .output();
    match res {
        Ok(o) if o.status.success() => {
            std::fs::rename(&part, &final_path).map_err(|e| {
                let _ = std::fs::remove_file(&part);
                format!("rename: {e}")
            })?;
            Ok(final_path)
        }
        Ok(o) => {
            let _ = std::fs::remove_file(&part);
            let err = String::from_utf8_lossy(&o.stderr);
            let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            Err(format!("ffmpeg failed: {tail}"))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(format!("ffmpeg: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_list_encodes_spaces_and_arabic() {
        let s = uri_list(&["/home/u/Videos/My Clip.mp4", "/tmp/لعبة.mp4"]);
        assert_eq!(
            s,
            "file:///home/u/Videos/My%20Clip.mp4\r\n\
             file:///tmp/%D9%84%D8%B9%D8%A8%D8%A9.mp4\r\n"
        );
        assert_eq!(uri_list::<&str>(&[]), "");
        assert!(uri_list(&["/a/100%#?.mp4"]).starts_with("file:///a/100%25%23%3F.mp4"));
    }
}
