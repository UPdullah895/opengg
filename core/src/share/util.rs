//! Small filesystem / encoding helpers shared by the share submodules.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Percent-encode `s` as UTF-8, leaving only RFC 3986 unreserved characters
/// (and any byte in `keep`) as-is. Used for file:// URIs (keep `/`) and query
/// strings (keep nothing).
pub fn percent_encode(s: &str, keep: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') || keep.contains(&b)
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `name.mp4`, or the first free `name (2).mp4` … `name (49).mp4`. Never
/// overwrites — a previous share of the same clip may still be on the
/// clipboard or mid-upload in a browser tab.
pub fn unique_path(p: &Path) -> Result<PathBuf, String> {
    if !p.exists() {
        return Ok(p.to_path_buf());
    }
    let dir = p.parent().unwrap_or_else(|| Path::new("."));
    let stem = p
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    let ext = p
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 2..=49 {
        let cand = dir.join(format!("{stem} ({n}){ext}"));
        if !cand.exists() {
            return Ok(cand);
        }
    }
    Err(format!(
        "Too many files named {stem}{ext} in {}",
        dir.display()
    ))
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn hex_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed) as u64;
    let v = nanos ^ ((std::process::id() as u64) << 20) ^ n.wrapping_mul(0x9E37_79B9);
    format!("{:08x}", v as u32)
}

/// Hidden, unique temp name next to the final file: `.<stem>.<hex>.part.mp4`.
/// Writing beside the target keeps the final `rename` atomic (same
/// filesystem), and the dot-prefix hides half-written files from file
/// managers and the clip watcher.
pub fn part_path(final_path: &Path) -> PathBuf {
    let dir = final_path.parent().unwrap_or_else(|| Path::new("."));
    let stem = final_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "clip".into());
    dir.join(format!(".{stem}.{}.part.mp4", hex_token()))
}

/// Is `bin` an executable on PATH?
pub fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

/// Delete regular files in `dir` (non-recursive) last modified more than
/// `max_age` ago. Returns how many were removed. Errors are ignored: pruning
/// a cache must never fail a job.
pub fn prune_old(dir: &Path, max_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut n = 0;
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        let old = md
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age > max_age);
        if old && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

/// ffmpeg time argument with millisecond precision.
pub fn secs_arg(s: f64) -> String {
    format!("{:.3}", s.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encoding() {
        assert_eq!(percent_encode("a b/c", b"/"), "a%20b/c");
        assert_eq!(percent_encode("a b/c", b""), "a%20b%2Fc");
        assert_eq!(percent_encode("é", b""), "%C3%A9");
        assert_eq!(percent_encode("A-z_0.~", b""), "A-z_0.~");
    }

    #[test]
    fn unique_and_part_paths() {
        let dir = std::env::temp_dir().join(format!("opengg-share-util-{}", hex_token()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("clip.mp4");
        assert_eq!(unique_path(&p).unwrap(), p);
        std::fs::write(&p, b"x").unwrap();
        assert_eq!(unique_path(&p).unwrap(), dir.join("clip (2).mp4"));
        let part = part_path(&p);
        let name = part.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(".clip.") && name.ends_with(".part.mp4"),
            "{name}"
        );
        assert_ne!(part_path(&p), part);
        assert_eq!(prune_old(&dir, Duration::from_secs(3600)), 0);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(prune_old(&dir, Duration::from_millis(1)), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
