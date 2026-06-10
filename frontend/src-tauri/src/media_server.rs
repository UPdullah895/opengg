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

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio rt");

        rt.block_on(async move {
            // Canonicalize allowed roots: clip directories + thumbnails directory
            let thumbnail_dir = dirs::data_local_dir()
                .map(|d| d.join("opengg").join("thumbnails"));
            let mut allowed_roots: Vec<PathBuf> = clip_dirs
                .iter()
                .filter_map(|d| d.canonicalize().ok())
                .collect();
            if let Some(td) = thumbnail_dir {
                if let Ok(canon_td) = td.canonicalize() {
                    allowed_roots.push(canon_td);
                } else if let Ok(canon_td) = std::fs::canonicalize(&td) {
                    allowed_roots.push(canon_td);
                }
            }
            let allowed_roots = Arc::new(allowed_roots);
            let token = Arc::new(token_clone);

            // Route 1: /media/... → serve files directly (video playback with Range)
            // Token required in query string. Validates canonical path is within allowed roots.
            // After validation, delegates to warp's file responder for Range request support.
            let media_route = warp::path("media")
                .and(warp::query::<TokenQuery>())
                .and(warp::path::tail())
                .and(warp::header::optional::<String>("range"))
                .and_then({
                    let allowed_roots = allowed_roots.clone();
                    let token = token.clone();
                    move |q: TokenQuery, tail: warp::path::Tail, range: Option<String>| {
                        let allowed_roots = allowed_roots.clone();
                        let token = token.clone();
                        async move { serve_media_with_auth(&allowed_roots, &token, q, tail, range).await }
                    }
                });

            // Route 2: /audio?file=/path/to/clip.mkv&stream=1
            // Extracts a single audio stream via ffmpeg → serves as audio/wav
            let audio_route = warp::path("audio")
                .and(warp::query::<AudioQuery>())
                .and_then({
                    let allowed_roots = allowed_roots.clone();
                    let token = token.clone();
                    move |q: AudioQuery| {
                        let allowed_roots = allowed_roots.clone();
                        let token = token.clone();
                        async move { serve_audio_stream_with_auth(&allowed_roots, &token, q).await }
                    }
                });

            // Route 3: /ext/<extId>/<rest> → serves static assets from the extensions directory.
            // Only files inside ~/.local/share/opengg/extensions/ are accessible.
            let ext_route = warp::path("ext")
                .and(warp::query::<TokenQuery>())
                .and(warp::path::tail())
                .and_then({
                    let token = token.clone();
                    move |q: TokenQuery, tail: warp::path::Tail| {
                        let token = token.clone();
                        async move { serve_extension_asset_with_auth(&token, q, tail).await }
                    }
                });

            let routes = media_route.or(audio_route).or(ext_route);

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

// ── Query and auth types ──────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

#[derive(Debug)]
struct AuthError;
impl warp::reject::Reject for AuthError {}

async fn serve_media_with_auth(
    allowed_roots: &[PathBuf],
    token: &str,
    q: TokenQuery,
    tail: warp::path::Tail,
    range: Option<String>,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    if q.token.as_ref() != Some(&token.to_string()) {
        return Err(warp::reject::custom(AuthError));
    }

    let tail_str = tail.as_str();
    if tail_str.is_empty() {
        return Err(warp::reject::not_found());
    }

    let decoded = percent_decode_str(tail_str).decode_utf8_lossy().to_string();
    let candidate = PathBuf::from(&decoded);

    let real = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(warp::reject::not_found()),
    };

    // Check if the real path is under one of the allowed roots
    let is_allowed = allowed_roots.iter().any(|root| real.starts_with(root));
    if !is_allowed {
        return Err(warp::reject::not_found());
    }

    if !real.is_file() {
        return Err(warp::reject::not_found());
    }

    let metadata = tokio::fs::metadata(&real)
        .await
        .map_err(|_| warp::reject::not_found())?;
    let total_size = metadata.len();

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
                .header("Content-Type", "application/octet-stream")
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
        .header("Content-Type", "application/octet-stream")
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

#[derive(serde::Deserialize)]
struct AudioQuery {
    token: Option<String>,
    file: String,
    stream: u32,
}

async fn serve_audio_stream_with_auth(
    allowed_roots: &[PathBuf],
    token: &str,
    q: AudioQuery,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    if q.token.as_ref() != Some(&token.to_string()) {
        return Err(warp::reject::custom(AuthError));
    }

    let decoded = percent_decode_str(&q.file).decode_utf8_lossy().to_string();
    let candidate = PathBuf::from(&decoded);

    let real = match candidate.canonicalize() {
        Ok(p) => p,
        Err(_) => return Err(warp::reject::not_found()),
    };

    // Check if the real path is under one of the allowed roots
    let is_allowed = allowed_roots.iter().any(|root| real.starts_with(root));
    if !is_allowed {
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
            &format!("0:{}", q.stream),
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
    q: TokenQuery,
    tail: warp::path::Tail,
) -> Result<Response<Vec<u8>>, warp::Rejection> {
    if q.token.as_ref() != Some(&token.to_string()) {
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
        return Err(warp::reject::not_found());
    }

    if !real.is_file() {
        return Err(warp::reject::not_found());
    }

    let bytes = tokio::fs::read(&real)
        .await
        .map_err(|_| warp::reject::not_found())?;

    // Derive Content-Type from extension
    let ct = match real.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "css" => "text/css; charset=utf-8",
        _ => "application/octet-stream",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", ct)
        .header("Content-Length", bytes.len().to_string())
        .header("Cache-Control", "no-cache")
        .body(bytes)
        .unwrap())
}
