//! Local HTTP Media Server — serves video files and individual audio tracks.
//!
//! Routes:
//!   /media/...     → serves files with Range support (video playback)
//!   /audio?file=X&stream=N → extracts audio stream N from X via ffmpeg, serves as WAV

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

            let routes = media_route.or(audio_route);

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
