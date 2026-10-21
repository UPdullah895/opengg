//! Non-destructive multi-part clip editing.
//!
//! - [`project`]: the edit model ([`EditProject`] / [`Part`]), source↔output
//!   time mapping and functional edit operations (`Option<EditProject>`
//!   results so only real changes reach the undo history).
//! - [`history`]: snapshot undo/redo with drag coalescing ([`History`]).
//! - [`looks`]: colour presets rendered as ffmpeg filters ([`Look`]).
//! - [`graph`]: pure `EditProject` → `-filter_complex` builder.
//! - [`export`]: blocking, cancellable ffmpeg CLI runner.
//!
//! The source clip is never modified; an edit only materialises as a new
//! file when exported. Everything except `export` is pure and unit-tested.

pub mod export;
pub mod graph;
pub mod history;
pub mod looks;
pub mod project;

pub use export::{export, Encoder, ExportOptions};
pub use graph::{build as build_graph, FilterGraph, GraphOptions};
pub use history::{History, HISTORY_CAP};
pub use looks::{filter as look_filter, Look};
pub use project::{EditProject, Part, MAX_LEVEL, MAX_SPEED, MIN_PIECE, MIN_SPEED};
