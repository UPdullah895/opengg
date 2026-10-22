//! "Share" exports: make a file that fits a platform's upload limits at the
//! best quality those limits allow, then get it to the platform with as little
//! friction as possible (clipboard, caption with hashtags, upload page).
//!
//! - [`planner`] — pure maths: platform specs, trim window, 9:16 crop,
//!   resolution ladder, bitrate/fps choice, size estimate, retry rule.
//! - [`transcode`] — probe + plan for the QML preview, and the ffmpeg render
//!   (hardware encoder fallback chain, progress, cancel, size-cap retries).
//! - [`clipboard`] — copy files as `text/uri-list`; lossless "copy a part".
//! - [`hashtags`] — game hashtag, caption, remembered tags, share URLs.

pub mod clipboard;
pub mod hashtags;
pub mod planner;
pub mod transcode;
mod util;

pub use clipboard::{copy_files_to_clipboard, lossless_cut, uri_list};
pub use hashtags::{caption, extract_tags, game_tag, remember_tags, upload_page, x_intent_url};
pub use planner::{
    format_bytes, strictest, Crop, LadderStep, Platform, Spec, MAX_RETRIES, TOO_LONG_ERROR,
};
pub use transcode::{
    available_encoders, default_output_path, plan, plan_with_source, probe_source, render,
    share_cache_root, Encoder, SharePlan, SharePlanRequest, ShareTrack, SourceInfo, CANCELLED,
};
pub use util::prune_old;
