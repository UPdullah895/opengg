//! Local HTTP Media Server — serves video files and individual audio tracks.
//!
//! Routes:
//!   /media/...              → serves files with Range support (video playback)
//!   /audio?file=X&stream=N  → extracts audio stream N from X via ffmpeg, serves as WAV
//!   /ext/<extId>/<path>     → serves static files from the extensions directory (icons, IIFE bundles, locales)

use percent_encoding::percent_decode_str;
use std::net::TcpListener;
use std::path::PathBuf;
use warp::http::{Response, StatusCode};
use warp::Filter;

fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind port 0")
        .local_addr()
        .expect("local_addr")
        .port()
}

pub fn spawn_media_server() -> u16 {
    let port = find_available_port();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio rt");

        rt.block_on(async move {
            // Route 1: /media/... → serve files directly (video playback with Range)
            let media_route = warp::path("media")
                .and(warp::fs::dir("/"));

            // Route 2: /audio?file=/path/to/clip.mkv&stream=1
            // Extracts a single audio stream via ffmpeg → serves as audio/wav
            let audio_route = warp::path("audio")
                .and(warp::query::<AudioQuery>())
                .and_then(serve_audio_stream);

            // Route 3: /ext/<extId>/<rest> → serves static assets from the extensions directory.
            // Only files inside ~/.local/share/opengg/extensions/ are accessible.
            let ext_route = warp::path("ext")
                .and(warp::path::tail())
                .and_then(serve_extension_asset);

            let routes = media_route.or(audio_route).or(ext_route);

            let cors = warp::cors()
                .allow_any_origin()
                .allow_methods(vec!["GET", "HEAD", "OPTIONS"])
                .allow_headers(vec![
                    "Range", "Content-Type", "Accept",
                    "If-Range", "If-Modified-Since", "If-None-Match",
                ]);

            warp::serve(routes.with(cors))
                .run(([127, 0, 0, 1], port))
                .await;
        });
    });

    port
}

#[derive(serde::Deserialize)]
struct AudioQuery {
    file: String,
    stream: u32,
}

/// Extract a single audio stream from a media file via ffmpeg.
/// Returns WAV data for the HTML5 <audio> element.
async fn serve_audio_stream(q: AudioQuery) -> Result<Response<Vec<u8>>, warp::Rejection> {
    let decoded = percent_decode_str(&q.file).decode_utf8_lossy().to_string();
    let path = PathBuf::from(&decoded);

    if !path.exists() || !path.is_file() {
        return Err(warp::reject::not_found());
    }

    // Security check
    let ps = path.to_string_lossy();
    if !(ps.starts_with("/home/") || ps.starts_with("/tmp/") || ps.contains(".local/share/opengg/")) {
        return Err(warp::reject::not_found());
    }

    // Use ffmpeg to extract the specific audio stream as WAV
    let output = tokio::process::Command::new("ffmpeg")
        .args([
            "-i", &decoded,
            "-map", &format!("0:{}", q.stream),
            "-ac", "2",        // stereo
            "-ar", "48000",    // standard sample rate
            "-f", "wav",       // WAV format for universal browser support
            "-vn",             // no video
            "pipe:1",          // output to stdout
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
async fn serve_extension_asset(tail: warp::path::Tail) -> Result<Response<Vec<u8>>, warp::Rejection> {
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

    let bytes = tokio::fs::read(&real).await.map_err(|_| warp::reject::not_found())?;

    // Derive Content-Type from extension
    let ct = match real.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "js"   | "mjs"  => "application/javascript; charset=utf-8",
        "json"           => "application/json; charset=utf-8",
        "svg"            => "image/svg+xml",
        "png"            => "image/png",
        "jpg" | "jpeg"   => "image/jpeg",
        "webp"           => "image/webp",
        "css"            => "text/css; charset=utf-8",
        _                => "application/octet-stream",
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", ct)
        .header("Content-Length", bytes.len().to_string())
        .header("Cache-Control", "no-cache")
        .body(bytes)
        .unwrap())
}
