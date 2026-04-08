/// OpenGG DSP Effects Engine — EasyEffects-inspired architecture
///
/// Architecture overview:
/// ──────────────────────────────────────────────────────────────
/// Each virtual sink gets a FilterGraph that chains PipeWire filter nodes:
///
///   [app sink-input] → [pw-loopback passthrough] → [LV2 EQ] → [LV2 Compressor]
///                                                → [rnnoise] → [output sink]
///
/// PipeWire filter chains use `pw-loopback` nodes wired together with the
/// PipeWire session manager (WirePlumber rules or direct pw-link calls).
///
/// LV2 plugins (compressor, EQ, gate) are hosted via the `lilv` crate.
/// Add to daemon/Cargo.toml under [dependencies]:
///   lilv = { version = "0.6", optional = true }
///   [features]
///   lv2 = ["lilv"]
///
/// rnnoise is loaded as a PipeWire module rather than an LV2 plugin:
///   pactl load-module module-echo-cancel aec_method=rnnoise

use std::process::Command;

// ── LV2 plugin URIs (Linux Studio Plugins) ────────────────────────────────
pub const LSP_COMPRESSOR_URI: &str = "http://lsp-plug.in/plugins/lv2/compressor_stereo";
pub const LSP_PARA_EQ_URI:    &str = "http://lsp-plug.in/plugins/lv2/para_equalizer_x16_stereo";
pub const LSP_GATE_URI:       &str = "http://lsp-plug.in/plugins/lv2/gate_stereo";

/// A single node in the DSP filter chain.
/// In production, this wraps a PipeWire filter node created via `pw-cli`
/// or libpipewire, with the LV2 plugin loaded as a SPA plugin.
#[allow(dead_code)]
pub struct FilterNode {
    /// Human-readable label for logging / debug UI
    pub label: String,
    /// LV2 plugin URI (None for built-in PW modules like rnnoise)
    pub lv2_uri: Option<String>,
    /// PipeWire object ID assigned after node creation (0 = not yet created)
    pub pw_object_id: u32,
}

/// A complete DSP chain for one virtual sink channel.
///
/// Activation sequence:
///   1. `add_lv2_node()` / `add_rnnoise_node()` — register desired nodes
///   2. `activate()` — instantiate PW filter nodes and wire them in order
///   3. `deactivate()` — unload all PW filter nodes
#[derive(Default)]
pub struct FilterGraph {
    pub channel: String,
    nodes: Vec<FilterNode>,
}

impl FilterGraph {
    pub fn new(channel: impl Into<String>) -> Self {
        Self { channel: channel.into(), nodes: Vec::new() }
    }

    /// Register an LV2 plugin node.
    /// The plugin must be installed on the host (e.g. lsp-plugins, calf).
    pub fn add_lv2_node(&mut self, label: impl Into<String>, uri: impl Into<String>) {
        self.nodes.push(FilterNode {
            label: label.into(),
            lv2_uri: Some(uri.into()),
            pw_object_id: 0,
        });
    }

    /// Register a rnnoise noise-reduction node.
    /// Implemented via PipeWire's module-echo-cancel with aec_method=rnnoise.
    pub fn add_rnnoise_node(&mut self) {
        self.nodes.push(FilterNode {
            label: "rnnoise".into(),
            lv2_uri: None,
            pw_object_id: 0,
        });
    }

    /// Wire the filter chain into PipeWire.
    ///
    /// Production implementation would:
    ///   - Use `libpipewire` (pw-sys crate) to create filter nodes
    ///   - Load each LV2 plugin via spa_handle_factory with
    ///     "lv2" as the factory name and the plugin URI as a property
    ///   - Link nodes sequentially with pw_link_create()
    ///
    /// Current stub uses `pactl` / `pw-link` CLI for simplicity.
    pub fn activate(&mut self) -> Result<(), String> {
        for node in &mut self.nodes {
            let pw_id = if node.lv2_uri.is_none() {
                // rnnoise: load as PipeWire echo-cancel module
                load_rnnoise_module(&self.channel)?
            } else {
                // LV2: spawn a pw-jack / jalv bridge as a subprocess
                // (replace with native libpipewire binding in production)
                spawn_lv2_node(&self.channel, node.lv2_uri.as_deref().unwrap_or(""))?
            };
            node.pw_object_id = pw_id;
            eprintln!("[opengg] FilterGraph[{}]: activated node '{}' → pw_id={}", self.channel, node.label, pw_id);
        }
        Ok(())
    }

    /// Remove all filter nodes from PipeWire.
    pub fn deactivate(&mut self) {
        for node in &mut self.nodes {
            if node.pw_object_id > 0 {
                unload_pw_module(node.pw_object_id);
                node.pw_object_id = 0;
            }
        }
    }
}

// ── Low-level helpers ─────────────────────────────────────────────────────

/// Load rnnoise via PipeWire's echo-cancel module.
/// Returns the pactl module index for later unloading.
fn load_rnnoise_module(channel: &str) -> Result<u32, String> {
    let sink_name = format!("opengg_{}", channel.to_lowercase());
    let out = Command::new("pactl")
        .args([
            "load-module", "module-echo-cancel",
            &format!("sink_name={sink_name}_rnnoise"),
            "aec_method=rnnoise",
        ])
        .output()
        .map_err(|e| format!("pactl failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout.trim().parse::<u32>().map_err(|_| format!("unexpected pactl output: {stdout}"))
}

/// Stub: spawn jalv (LV2 host) wired to PipeWire via JACK bridge.
/// In production replace with native PipeWire SPA/LV2 integration.
fn spawn_lv2_node(_channel: &str, uri: &str) -> Result<u32, String> {
    // Production path:
    //   pw-jack jalv -i <uri> &
    //   then use pw-cli to find and wire the new node
    eprintln!("[opengg] spawn_lv2_node: stub — LV2 native PW integration not yet implemented. uri={uri}");
    Ok(0)
}

/// Unload a PipeWire module by its index.
fn unload_pw_module(index: u32) {
    if index == 0 { return; }
    let _ = Command::new("pactl").args(["unload-module", &index.to_string()]).status();
}

// ── Public API used by D-Bus handlers ─────────────────────────────────────

/// Build a default DSP chain for a microphone input channel (Chat/Mic).
/// Chain: rnnoise → gate (LSP) → compressor (LSP)
pub fn mic_chain(channel: &str) -> FilterGraph {
    let mut g = FilterGraph::new(channel);
    g.add_rnnoise_node();
    g.add_lv2_node("Gate",       LSP_GATE_URI);
    g.add_lv2_node("Compressor", LSP_COMPRESSOR_URI);
    g
}

/// Build a default EQ chain for an output channel (Game/Chat/Media/Aux).
/// Chain: parametric EQ (LSP 16-band)
pub fn output_eq_chain(channel: &str) -> FilterGraph {
    let mut g = FilterGraph::new(channel);
    g.add_lv2_node("ParaEQ", LSP_PARA_EQ_URI);
    g
}
