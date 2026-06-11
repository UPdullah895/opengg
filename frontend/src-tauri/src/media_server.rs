//! Local HTTP Media Server — serves video files and individual audio tracks.
//!
//! Routes:
//!   /media/...              → serves files with Range support (video playback)
//!   /audio?file=X&stream=N  → extracts audio stream N from X via ffmpeg, serves as WAV
//!   /ext/<extId>/<path>     → serves static files from the extensions directory (icons, IIFE bundles, locales)
//!
//! All routes require a valid session token as a ?token=X query parameter.
//! Files outside allowed directories (clip directories + thumbnail directory) are rejected.

use percent_encoding::percent_decode_str;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use warp::http::{Response, StatusCode};
use warp::Filter;
use warp::Rejection;

fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind port 0")
        .local_addr()
        .expect("local_addr")
        .port()
}

/// Generate a random 32-character hex session token (128 bits) for authenticating media server requests.
fn generate_session_token() -> String {
    use std::io::Read;
    let mut buf = [0u8; 16];

    // Try reading from /dev/urandom first
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut buf).is_ok() {
            // Convert 16 bytes to 32 hex characters
            return buf.iter().map(|b| format!("{:02x}", b)).collect();
        }
    }

    // Fallback: use a combination of current time and RandomState for less critical entropy
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(now);
    let h1 = hasher.finish();

    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(now ^ 0xdeadbeef);
    let h2 = hasher.finish();

    format!("{:016x}{:016x}", h1, h2)
}

pub fn spawn_media_server(clip_dirs: Vec<PathBuf>) -> (u16, String) {
    let port = find_available_port();
    let token = generate_session_token();
    let token_clone = token.clone();

    // Create all clip directories and thumbnails directory at startup to ensure they exist
    for dir in &clip_dirs {
        if let Err(e) = std::fs::create_dir_all(dir) {
            log::warn!("Failed to create clip directory {}: {}", dir.display(), e);
        }
    }
    if let Some(data_dir) = dirs::data_local_dir() {
        let thumbnail_dir = data_dir.join("opengg").join("thumbnails");
        if let Err(e) = std::fs::create_dir_all(&thumbnail_dir) {
            log::warn!("Failed to create thumbnail directory {}: {}", thumbnail_dir.display(), e);
        }
    }

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio rt");

        rt.block_on(async move {
            // Store allowed roots as raw PathBufs; canonicalize at request time.
            // This allows directories created after launch to work without restart.
            let mut allowed_roots: Vec<PathBuf> = clip_dirs.clone();
            if let Some(data_dir) = dirs::data_local_dir() {
                allowed_roots.push(data_dir.join("opengg").join("thumbnails"));
            }
            let allowed_roots = Arc::new(allowed_roots);
            let token = Arc::new(token_clone);

            // Route 1: /media/... → serve files directly (video playback with Range)
            // Token required in query string. Validates canonical path is within allowed roots.
            // Uses tolerant query parsing: a request with no ?query=... string is treated as token=None
            // and gets 401 Unauthorized (not 500 rejected).
            let media_route = warp::path("media")
                .and(warp::query::raw().or(warp::any().map(String::new)).unify())
                .and(warp::path::tail())
                .and(warp::header::optional::<String>("range"))
                .and_then({
                    let allowed_roots = allowed_roots.clone();
                    let token = token.clone();
                    move |raw_query: String, tail: warp::path::Tail, range: Option<String>| {
                        let allowed_roots = allowed_roots.clone();
                        let token = token.clone();
                        async move {
                            serve_media_with_auth(&allowed_roots, &token, raw_query, tail, range).await
                        }
                    }
                });

            // Route 2: /audio?file=/path/to/clip.mkv&stream=1
            // Extracts a single audio stream via ffmpeg → serves as audio/wav
            // Tolerant query parsing: missing query or missing token both → 401 Unauthorized
            let audio_route = warp::path("audio")
                .and(warp::query::raw().or(warp::any().map(String::new)).unify())
                .and_then({
                    let allowed_roots = allowed_roots.clone();
                    let token = token.clone();
                    move |raw_query: String| {
                        let allowed_roots = allowed_roots.clone();
                        let token = token.clone();
                        async move { serve_audio_stream_with_auth(&allowed_roots, &token, raw_query).await }
                    }
                });

            // Route 3: /ext/<extId>/<rest> → serves static assets from the extensions directory.
            // Only files inside ~/.local/share/opengg/extensions/ are accessible.
            // Tolerant query parsing: missing query or missing token both → 401 Unauthorized
            let ext_route = warp::path("ext")
                .and(warp::query::raw().or(warp::any().map(String::new)).unify())
                .and(warp::path::tail())
                .and_then({
                    let token = token.clone();
                    move |raw_query: String, tail: warp::path::Tail| {
                        let token = token.clone();
                        async move { serve_extension_asset_with_auth(&token, raw_query, tail).await }
                    }
                });

            // recover() must wrap the COMBINED routes, not each route: a per-route
            // recover would convert a path mismatch (not_found rejection) into a
            // 404 response, short-circuiting `.or()` so /audio and /ext never match.
            let routes = media_route
                .or(audio_route)
                .or(ext_route)
                .recover(handle_rejection);

            let cors = warp::cors()
                .allow_any_origin()
                .allow_methods(vec!["GET", "HEAD", "OPTIONS"])
                .allow_headers(vec![
                    "Range",
                    "Content-Type",
                    "Accept",
                    "If-Range",
                    "If-Modified-Since",
                    "If-None-Match",
                ]);

            warp::serve(routes.with(cors))
                .run(([127, 0, 0, 1], port))
                .await;
        });
    });

    (port, token)
}

// ── Auth types ───────────────────────────────────────────────────────

#[derive(Debug)]
struct AuthError;
impl warp::reject::Reject for AuthError {}

/// Extract token from raw query string, tolerating missing query altogether.
/// Format: ?token=<value> or no ? at all.
/// Returns the token value (percent-decoded) or None if absent/empty.
fn extract_token_from_raw_query(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    // Split on '&' to find all parameters
    for param in raw.split('&') {
        if let Some(value) = param.strip_prefix("token=") {
            if !value.is_empty() {
                // Percent-decode the value
                return Some(percent_decode_str(value).decode_utf8_lossy().to_string());
            }
        }
    }
    None
}

/// Extension-to-MIME-type mapping with fallback to application/octet-stream.
fn mime_type_for_extension(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        // Video
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "mov" => "video/quicktime",
        "ts" => "video/mp2t",
        "flv" => "video/x-flv",
        // Image
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        // Audio
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        // Web
        "json" => "application/json",
        "js" | "mjs" => "application/javascript",
        "css" => "text/css",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
}

/// Handle warp rejections: AuthError → 401, not_found → 404, others → 500
async fn handle_rejection(err: Rejection) -> Result<impl warp::Reply, Rejection> {
    if err.is_not_found() {
        return Ok(warp::reply::with_status(
            "Not Found",
            StatusCode::NOT_FOUND,
        ));
    }
    if let Some(AuthError) = err.find() {
        log::debug!("Authentication failed: invalid or missing token");
        return Ok(warp::reply::with_status(
            "Unauthorized",
            StatusCode::UNAUTHORIZED,
        ));
    }
    // Log other rejections (unmatched routes, etc.)
    log::debug!("Request rejected: {:?}", err);
    Err(err)
}

async fn serve_media_with_auth(
    allowed_roots: &[PathBuf],
    token: &str,
    raw_query: String,
    tail: warp::path::Tail,
    range: Option<String>,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    let provided_token = extract_token_from_raw_query(&raw_query);
    if provided_token.as_ref() != Some(&token.to_string()) {
        log::debug!("Media auth failed: expected token, got {:?}", provided_token);
        return Err(warp::reject::custom(AuthError));
    }

    let tail_str = tail.as_str();
    if tail_str.is_empty() {
        return Err(warp::reject::not_found());
    }

    let decoded = percent_decode_str(tail_str).decode_utf8_lossy().to_string();
    // The warp tail has no leading slash ("home/user/Videos/…"), so building a
    // PathBuf from it yields a RELATIVE path that canonicalizes against the
    // process CWD and 404s every file. Re-root it onto / (the old
    // warp::fs::dir("/") did this implicitly).
    let candidate = std::path::Path::new("/").join(&decoded);

    // Canonicalize at request time, not startup, so dynamically created dirs work
    let real = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(warp::reject::not_found()),
    };

    // Check if the real path is under one of the allowed roots.
    // Canonicalize each root at request time (cheap: a few syscalls per request, localhost-only).
    let is_allowed = allowed_roots.iter().any(|root| {
        match root.canonicalize() {
            Ok(canon_root) => real.starts_with(&canon_root),
            Err(_) => {
                log::debug!("Could not canonicalize allowed root: {}", root.display());
                false
            }
        }
    });
    if !is_allowed {
        log::debug!("Path not in allowed roots: {}", real.display());
        return Err(warp::reject::not_found());
    }

    if !real.is_file() {
        return Err(warp::reject::not_found());
    }

    let metadata = tokio::fs::metadata(&real)
        .await
        .map_err(|_| warp::reject::not_found())?;
    let total_size = metadata.len();

    let content_type = mime_type_for_extension(&real);

    // Parse Range header if present and valid
    if let Some(range_header) = range {
        if let Some((start, end)) = parse_range_header(&range_header, total_size) {
            let bytes = tokio::fs::read(&real)
                .await
                .map_err(|_| warp::reject::not_found())?;

            let range_bytes = bytes[start..=end].to_vec();
            let content_length = range_bytes.len();

            return Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header("Content-Type", content_type)
                .header("Content-Length", content_length.to_string())
                .header(
                    "Content-Range",
                    format!("bytes {}-{}/{}", start, end, total_size),
                )
                .header("Accept-Ranges", "bytes")
                .body(range_bytes)
                .unwrap());
        }
    }

    // No Range header or invalid Range — serve full file
    let bytes = tokio::fs::read(&real)
        .await
        .map_err(|_| warp::reject::not_found())?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Length", bytes.len().to_string())
        .header("Accept-Ranges", "bytes")
        .body(bytes)
        .unwrap())
}

/// Parse a Range header value (e.g., "bytes=0-1023") and return (start, end) indices.
/// Returns None if the header is malformed or out of bounds.
fn parse_range_header(header: &str, total_size: u64) -> Option<(usize, usize)> {
    // Expected format: "bytes=start-end"
    if !header.starts_with("bytes=") {
        return None;
    }

    let range_part = &header[6..]; // skip "bytes="
    let parts: Vec<&str> = range_part.split('-').collect();

    if parts.len() != 2 {
        return None;
    }

    let start_str = parts[0].trim();
    let end_str = parts[1].trim();

    // Parse start
    let start = if let Ok(s) = start_str.parse::<u64>() {
        s
    } else {
        return None;
    };

    // Parse end
    let end = if let Ok(e) = end_str.parse::<u64>() {
        e
    } else {
        return None;
    };

    // Validate ranges
    if start >= total_size || end >= total_size || start > end {
        return None;
    }

    Some((start as usize, end as usize))
}

async fn serve_audio_stream_with_auth(
    allowed_roots: &[PathBuf],
    token: &str,
    raw_query: String,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    // Parse audio query from raw query string: ?file=<path>&stream=<num>&token=<token>
    let provided_token = extract_token_from_raw_query(&raw_query);
    if provided_token.as_ref() != Some(&token.to_string()) {
        log::debug!("Audio auth failed: expected token, got {:?}", provided_token);
        return Err(warp::reject::custom(AuthError));
    }

    // Extract file path and stream number
    let mut file_path: Option<String> = None;
    let mut stream_num: Option<u32> = None;

    for param in raw_query.split('&') {
        if let Some(value) = param.strip_prefix("file=") {
            if !value.is_empty() {
                file_path = Some(percent_decode_str(value).decode_utf8_lossy().to_string());
            }
        }
        if let Some(value) = param.strip_prefix("stream=") {
            if let Ok(num) = value.parse::<u32>() {
                stream_num = Some(num);
            }
        }
    }

    let file_path = file_path.ok_or_else(warp::reject::not_found)?;
    let stream_num = stream_num.ok_or_else(warp::reject::not_found)?;

    let decoded = percent_decode_str(&file_path).decode_utf8_lossy().to_string();
    let candidate = PathBuf::from(&decoded);

    // Canonicalize at request time
    let real = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(warp::reject::not_found()),
    };

    // Check if the real path is under one of the allowed roots (canonicalize roots at request time)
    let is_allowed = allowed_roots.iter().any(|root| {
        match root.canonicalize() {
            Ok(canon_root) => real.starts_with(&canon_root),
            Err(_) => {
                log::debug!("Could not canonicalize allowed root: {}", root.display());
                false
            }
        }
    });
    if !is_allowed {
        log::debug!("Audio path not in allowed roots: {}", real.display());
        return Err(warp::reject::not_found());
    }

    if !real.is_file() {
        return Err(warp::reject::not_found());
    }

    // Use ffmpeg to extract the specific audio stream as WAV
    let real_str = real.to_string_lossy().to_string();
    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-i",
            &real_str,
            "-map",
            &format!("0:{}", stream_num),
            "-ac",
            "2", // stereo
            "-ar",
            "48000", // standard sample rate
            "-f",
            "wav",    // WAV format for universal browser support
            "-vn",    // no video
            "pipe:1", // output to stdout
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .map_err(|_| warp::reject::not_found())?;

    if !output.status.success() || output.stdout.is_empty() {
        return Err(warp::reject::not_found());
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "audio/wav")
        .header("Content-Length", output.stdout.len().to_string())
        .header("Cache-Control", "public, max-age=3600")
        .body(output.stdout)
        .unwrap())
}

/// Serve a static asset from the extensions directory.
/// URL pattern: /ext/<extId>/<asset-path>
/// Maps to:     ~/.local/share/opengg/extensions/<extId>/<asset-path>
///
/// Path-traversal protection: resolves the full canonical path and verifies
/// it is strictly inside the extensions base directory before reading.
/// Requires valid session token as ?token=X query parameter.
async fn serve_extension_asset_with_auth(
    token: &str,
    raw_query: String,
    tail: warp::path::Tail,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    let provided_token = extract_token_from_raw_query(&raw_query);
    if provided_token.as_ref() != Some(&token.to_string()) {
        log::debug!("Extension auth failed: expected token, got {:?}", provided_token);
        return Err(warp::reject::custom(AuthError));
    }

    let tail_str = tail.as_str();
    if tail_str.is_empty() {
        return Err(warp::reject::not_found());
    }

    let base = match dirs::data_local_dir() {
        Some(d) => d.join("opengg").join("extensions"),
        None => return Err(warp::reject::not_found()),
    };

    // Decode percent-encoded segments from the URL
    let decoded = percent_decode_str(tail_str).decode_utf8_lossy().to_string();
    let candidate = base.join(&decoded);

    // Resolve symlinks and normalise '..' components to get the real path
    let real = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(warp::reject::not_found()),
    };

    // Ensure the resolved path starts with the (also canonicalised) extensions base
    let real_base = match base.canonicalize() {
        Ok(p) => p,
        Err(_) => base.clone(),
    };
    if !real.starts_with(&real_base) {
        log::debug!("Extension path not in allowed directory: {}", real.display());
        return Err(warp::reject::not_found());
    }

    if !real.is_file() {
        return Err(warp::reject::not_found());
    }

    let bytes = tokio::fs::read(&real)
        .await
        .map_err(|_| warp::reject::not_found())?;

    let ct = mime_type_for_extension(&real);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", ct)
        .header("Content-Length", bytes.len().to_string())
        .header("Cache-Control", "no-cache")
        .body(bytes)
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_token_from_raw_query() {
        // Empty query
        assert_eq!(extract_token_from_raw_query(""), None);

        // Token present and valid
        assert_eq!(
            extract_token_from_raw_query("token=abc123"),
            Some("abc123".to_string())
        );

        // Token with other parameters
        assert_eq!(
            extract_token_from_raw_query("file=/path&token=xyz789&stream=0"),
            Some("xyz789".to_string())
        );

        // Token at start
        assert_eq!(
            extract_token_from_raw_query("token=first&other=value"),
            Some("first".to_string())
        );

        // Percent-encoded token
        assert_eq!(
            extract_token_from_raw_query("token=abc%20123"),
            Some("abc 123".to_string())
        );

        // Empty token value
        assert_eq!(extract_token_from_raw_query("token="), None);

        // Missing token
        assert_eq!(extract_token_from_raw_query("file=/path&stream=0"), None);

        // Token-like but not matching
        assert_eq!(extract_token_from_raw_query("mytoken=value"), None);
    }

    #[test]
    fn test_mime_type_for_extension() {
        // Video types
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.mp4")), "video/mp4");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.mkv")), "video/x-matroska");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.webm")), "video/webm");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.avi")), "video/x-msvideo");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.mov")), "video/quicktime");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.ts")), "video/mp2t");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.flv")), "video/x-flv");

        // Image types
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.png")), "image/png");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.jpg")), "image/jpeg");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.jpeg")), "image/jpeg");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.webp")), "image/webp");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.gif")), "image/gif");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.svg")), "image/svg+xml");

        // Audio types
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.wav")), "audio/wav");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.mp3")), "audio/mpeg");

        // Web types
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.json")), "application/json");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.js")), "application/javascript");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.mjs")), "application/javascript");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.css")), "text/css");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.html")), "text/html");

        // Fallback
        assert_eq!(mime_type_for_extension(std::path::Path::new("file.unknown")), "application/octet-stream");
        assert_eq!(mime_type_for_extension(std::path::Path::new("file")), "application/octet-stream");
    }
}
