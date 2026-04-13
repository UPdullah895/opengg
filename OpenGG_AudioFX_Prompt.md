# OpenGG Audio Effects Engine — Full Specification Prompt

> **Context**: OpenGG is a Tauri 2 + Rust backend + Vue.js/Svelte frontend open-source Linux gaming companion app. This prompt defines a complete audio effects processing pipeline inspired by EasyEffects (PipeWire-based), designed to be implemented as part of OpenGG's Audio Mixer module. The goal is to give gamers professional-grade audio control over their microphone input and game/app output — all within a single, unified UI.

---

## 1. Architecture Overview

### Signal Flow
```
[PipeWire Source] → [Input Gain] → [Effects Chain (ordered, drag-reorderable)] → [Output Gain] → [PipeWire Sink]
```

### Two Processing Pipelines
- **Output Pipeline** — processes audio from applications (games, Discord, music) before it reaches speakers/headphones
- **Input Pipeline** — processes microphone audio before it reaches recording/communication apps

### Core Design Principles
- Effects are **modular plugins** — each can be enabled/disabled independently
- The user has **full control over effects order** via drag-and-drop
- Each effect has **per-channel or linked stereo** processing
- All settings are **saveable as presets** (JSON format) and shareable
- Real-time **VU meters and spectrum visualization** at input, output, and per-effect stages

---

## 2. Complete Effects Catalog

### 2.1 — Equalizer (Parametric EQ)
**Purpose**: Adjust volume of specific frequency bands to shape the tonal character of audio.

**Parameters**:
- `bands_count`: 1–32 bands
- `mode`: IIR (minimal phase, low latency) | FIR (linear phase, adds latency) | FFT (linear phase, adds latency)
- `balance`: L/R output balance (-1.0 to 1.0)
- `pitch_left` / `pitch_right`: frequency shift per channel in semitones
- `split_channels`: bool — apply different EQ per channel

**Per-Band Parameters**:
- `type`: Off | Bell | HighPass | HighShelf | LowPass | LowShelf | Notch | Resonance | AllPass
- `filter_mode`: RLC (BT/MT) | BWC (BT/MT) | LRX (BT/MT) | APO (DR) — filter topology
- `slope`: filter steepness (dB/octave)
- `frequency`: center frequency (Hz)
- `gain`: boost/cut (dB)
- `quality`: Q factor (bandwidth = frequency / quality)
- `solo`: bool
- `mute`: bool

**Utility Functions**:
- "Flat Response" — reset all gains to 0
- "Calculate Frequencies" — auto-distribute bands evenly across spectrum
- Import/export AutoEQ profiles

---

### 2.2 — Compressor
**Purpose**: Reduce dynamic range — make quiet parts louder and loud parts quieter.

**Parameters**:
- `attack_time`: ms — time to apply compression
- `release_time`: ms — time to restore gain
- `attack_threshold`: dB — level where compression kicks in
- `release_threshold`: dB — offset from attack threshold for release behavior
- `ratio`: compression amount (e.g., 2:1, 4:1, ∞:1)
- `knee`: dB — smoothness of compression onset
- `makeup_gain`: dB — post-compression gain boost
- `dry_level` / `wet_level`: mix control
- `mode`: Downward | Upward | Boosting
- `boost_threshold`: dB (for Upward mode)
- `boost_amount`: dB (for Boosting mode)

**Sidechain**:
- `input_type`: Feed-forward | Feed-back | External (mic)
- `sc_mode`: Peak | RMS | Low-Pass | Uniform
- `sc_source`: Middle | Side | Left | Right | Min | Max
- `sc_preamp`: dB
- `sc_reactivity`: ms
- `sc_lookahead`: ms
- `sc_hp_filter`: mode + frequency
- `sc_lp_filter`: mode + frequency
- `sc_listen`: bool — audition the sidechain signal

---

### 2.3 — Limiter (Brick-Wall)
**Purpose**: Prevent signal from exceeding a ceiling — essential for clipping protection.

**Parameters**:
- `mode`: Hermite/Exponential/Linear × Thin/Tail/Duck/Wide (12 combinations)
- `oversampling`: None | Half(2x-8x) | Full(2x-8x) — improves peak detection
- `dither`: None | 16-bit | 24-bit
- `sc_preamp`: dB
- `lookahead`: ms — peak detection buffer
- `attack`: ms
- `release`: ms
- `threshold`: dB — ceiling
- `threshold_boost`: bool — auto-normalize to 0 dB
- `stereo_link`: 0–100%
- `external_sidechain`: bool

**Auto Leveling (ALR)**:
- `alr_enabled`: bool
- `alr_attack`: ms
- `alr_release`: ms
- `alr_knee`: dB — balance between peak-cutting and ALR gain reduction

---

### 2.4 — Gate (Noise Gate)
**Purpose**: Silence audio below a threshold — eliminates background noise when not speaking.

**Parameters**:
- `attack`: ms
- `release`: ms
- `threshold`: dB
- `ratio`: gating amount
- `knee`: dB
- `makeup`: dB
- `range`: dB — maximum gain reduction
- `hysteresis`: bool — separate open/close thresholds
- Sidechain options (same as Compressor)

---

### 2.5 — Multiband Compressor
**Purpose**: Apply independent compression to different frequency bands — surgical dynamic control.

**Parameters**:
- `bands_count`: 2–8
- Per band: `frequency_split`, `attack`, `release`, `threshold`, `ratio`, `knee`, `makeup`, `solo`, `mute`
- Global: `mode` (Downward/Upward), `dry/wet`
- Sidechain per band

---

### 2.6 — Multiband Gate
**Purpose**: Apply independent noise gating per frequency band.

**Parameters**: Same structure as Multiband Compressor but with Gate parameters per band.

---

### 2.7 — Auto Gain (Loudness Normalization)
**Purpose**: Automatically adjust volume to a target loudness level (EBU R 128 standard).

**Parameters**:
- `target`: LUFS — target loudness
- `silence_threshold`: LUFS — below this, no gain changes
- `max_history`: seconds — time window for loudness calculation
- `reference`: Momentary (400ms) | Short-Term (3s) | Integrated (long-term) | Geometric Mean

**Monitor Outputs** (read-only):
- Relative loudness, Current loudness, Loudness Range (LRA), Output Gain

---

### 2.8 — Noise Reduction (RNNoise)
**Purpose**: AI-based noise suppression using recurrent neural networks — removes keyboard clicks, fan noise, etc.

**Parameters**:
- `enabled`: bool
- `model`: default or custom .rnnn model file path
- Real-time noise/voice activity indicators

---

### 2.9 — Deep Noise Remover (DeepFilterNet)
**Purpose**: Advanced deep-learning noise removal — superior to RNNoise for complex noise environments.

**Parameters**:
- `attenuation_limit`: dB — maximum noise reduction
- Real-time processing indicators

---

### 2.10 — De-esser
**Purpose**: Reduce harsh sibilant sounds ("s", "sh", "t") in voice recordings.

**Parameters**:
- `frequency`: Hz — target sibilance frequency
- `threshold`: dB
- `ratio`: compression amount on sibilants
- `makeup`: dB
- `detection`: peak/RMS
- `laxity`: response smoothness
- `sc_listen`: bool — audition detected sibilants

---

### 2.11 — Echo Canceller
**Purpose**: Remove echo and feedback — useful when speakers and mic are in same room.

**Parameters**:
- `filter_length`: ms — echo tail length
- `frame_size`: ms

---

### 2.12 — Reverberation
**Purpose**: Add spatial room ambience to audio.

**Parameters**:
- `room_size`: 0–100%
- `decay_time`: seconds
- `hf_damp`: high-frequency damping
- `pre_delay`: ms
- `bass_cut`: Hz — roll off low frequencies
- `treble_cut`: Hz — roll off high frequencies
- `diffusion`: 0–100%
- `dry_level` / `wet_level`: mix
- `amount`: overall reverb intensity

---

### 2.13 — Delay
**Purpose**: Add time delay to one or both channels — for timing correction or creative effects.

**Parameters**:
- `time_left` / `time_right`: ms
- `dry_level` / `wet_level`: mix

---

### 2.14 — Convolver
**Purpose**: Apply impulse response (IR) files to simulate real acoustic spaces (rooms, halls, cabinets).

**Parameters**:
- `ir_file`: path to WAV/FLAC impulse response
- `ir_width`: stereo width of IR (0–200%)
- `dry_level` / `wet_level`: mix
- Built-in IR browser with preview

---

### 2.15 — Bass Enhancer
**Purpose**: Add harmonics to low frequencies for perceived bass boost without actual volume increase.

**Parameters**:
- `amount`: harmonic intensity
- `harmonics`: harmonic series mix
- `scope`: Hz — frequency below which enhancement is applied
- `floor`: Hz — lowest frequency affected
- `blend`: dry/wet mix
- `floor_active`: bool

---

### 2.16 — Bass Loudness
**Purpose**: Compensate for the ear's reduced sensitivity to bass at low listening volumes (Fletcher-Munson curve).

**Parameters**:
- `loudness`: perceived loudness level
- `output_level`: output gain
- `link`: link loudness to output level

---

### 2.17 — Exciter
**Purpose**: Add high-frequency harmonics for added "sparkle" and presence.

**Parameters**:
- `amount`: harmonic intensity
- `harmonics`: harmonic series
- `scope`: Hz — frequency above which to excite
- `ceil`: Hz — upper frequency limit
- `blend`: dry/wet
- `ceil_active`: bool

---

### 2.18 — Crystalizer
**Purpose**: Enhance dynamic contrast by amplifying differences between consecutive samples.

**Parameters**:
- `bands_count`: number of frequency bands
- Per band: `intensity` (%), `mute`, `bypass`

---

### 2.19 — Crusher (Bit Crusher)
**Purpose**: Intentional digital distortion — reduce bit depth and sample rate for lo-fi effect.

**Parameters**:
- `bit_depth`: 1–32 bits
- `sample_rate_reduction`: factor
- `anti_aliasing`: bool

---

### 2.20 — Crossfeed
**Purpose**: Mix small amount of opposite channel into each ear — reduces stereo fatigue on headphones.

**Parameters**:
- `fcut`: Hz — crossfeed cutoff frequency
- `feed`: dB — crossfeed level

---

### 2.21 — Pitch Shift
**Purpose**: Change the pitch of audio without changing speed.

**Parameters**:
- `semitones`: pitch shift amount (-12 to +12)
- `cents`: fine tuning
- `sequence`: ms — processing block size
- `overlap`: % — block overlap
- `seek_window`: ms — similarity search window

---

### 2.22 — Speech Processor (Speex)
**Purpose**: Optimize speech clarity — AGC, noise suppression, and voice activity detection.

**Parameters**:
- `agc_enabled`: bool + `agc_target_level`: dB
- `denoise_enabled`: bool + `noise_suppression`: dB
- `vad_enabled`: bool + `vad_probability`: 0–100%
- `dereverb_enabled`: bool + `dereverb_decay` / `dereverb_level`

---

### 2.23 — Filter (Simple)
**Purpose**: Basic frequency filtering — quick way to cut unwanted frequency ranges.

**Parameters**:
- `type`: Low-Pass | High-Pass | Band-Pass | Band-Reject
- `frequency`: Hz — cutoff/center
- `quality`: Q factor
- `gain`: dB (for shelf modes)
- `slope`: dB/octave

---

### 2.24 — Loudness (ISO 226)
**Purpose**: Apply equal-loudness contour compensation based on ISO 226 standard.

**Parameters**:
- `volume`: dB — reference playback level
- `clipping`: bool — enable/disable clipping prevention

---

### 2.25 — Maximizer
**Purpose**: Increase overall loudness while preventing clipping.

**Parameters**:
- `threshold`: dB
- `ceiling`: dB
- `release`: ms

---

### 2.26 — Expander (Upward/Downward)
**Purpose**: Increase dynamic range — opposite of compressor.

**Parameters**: Same structure as Compressor with `mode`: Downward | Upward

---

### 2.27 — Stereo Tools
**Purpose**: Advanced stereo field manipulation — width, balance, M/S processing.

**Input**:
- `balance`: L/R
- `softclip`: bool + level

**Stereo Matrix**:
- `mode`: Stereo | Mid-Side | Mono
- `mute_left` / `mute_right`
- `invert_phase_left` / `invert_phase_right`
- `side_level`: dB
- `side_balance`
- `middle_level`: dB
- `middle_panorama`

**Output**:
- `balance`
- `delay_lr`: ms (negative = delay left, positive = delay right)
- `stereo_base`: mono ↔ inverted
- `stereo_phase`

---

### 2.28 — Level Meter
**Purpose**: Visual monitoring — not an effect, but essential for the chain.

**Displays**:
- Peak level (dBFS)
- RMS level
- LUFS (Momentary, Short-term, Integrated)
- True Peak
- Phase correlation meter
- Spectrum analyzer (FFT)

---

### 2.29 — Spectrum Analyzer
**Purpose**: Real-time frequency domain visualization.

**Parameters**:
- `fft_size`: 256–8192
- `window`: Hann | Hamming | Blackman | Kaiser
- `display_mode`: Bars | Lines | Filled
- `smoothing`: 0–100%

---

## 3. Recommended Signal Chain Presets

### 🎤 Gaming Mic (Voice Chat)
```
Noise Gate → RNNoise/DeepFilter → De-esser → Compressor → EQ → Limiter
```

### 🎧 Game Audio (Output Enhancement)
```
EQ → Bass Enhancer → Compressor → Stereo Tools (Crossfeed) → Loudness → Limiter
```

### 🎵 Music Listening
```
EQ → Convolver (Room IR) → Bass Loudness → Crystalizer → Stereo Tools → Limiter
```

### 📺 Streaming/Recording
```
Noise Gate → DeepFilterNet → Speech Processor → Compressor → EQ → De-esser → Limiter → Auto Gain
```

---

## 4. Implementation Notes for OpenGG

### Backend (Rust/Tauri)
- Use **PipeWire native filters** via `pipewire-rs` crate for real-time audio processing
- Each effect = a Rust struct implementing a common `AudioEffect` trait:
  ```rust
  trait AudioEffect: Send + Sync {
      fn process(&mut self, buffer: &mut AudioBuffer);
      fn get_params(&self) -> serde_json::Value;
      fn set_params(&mut self, params: serde_json::Value);
      fn reset(&mut self);
      fn name(&self) -> &str;
      fn is_enabled(&self) -> bool;
  }
  ```
- Effects chain = `Vec<Box<dyn AudioEffect>>` with reorder support
- Use **LV2 plugin hosting** (via `lilv` or `lv2-host` crate) to leverage existing LSP/Calf/ZamAudio plugins instead of re-implementing DSP
- Alternative: use FFmpeg's audio filters (`-af`) for offline processing (export/render)
- Presets stored as JSON in `~/.config/opengg/audio_presets/`

### Frontend (Vue.js/Svelte)
- Each effect gets a collapsible panel with its controls
- Real-time VU meters using Web Audio API or WebSocket data from backend
- Drag-and-drop effect reordering
- Preset manager: save, load, import, export, community presets
- Per-application routing: choose which apps get effects applied

### Key Libraries & Dependencies
| Purpose | Library |
|---|---|
| PipeWire integration | `pipewire-rs` |
| LV2 plugin hosting | `lilv`, `lv2-host` |
| FFT/DSP | `rustfft`, `realfft` |
| Resampling | `rubato` |
| EBU R128 loudness | `ebur128` crate |
| RNNoise | `nnnoiseless` (pure Rust RNNoise port) |
| Impulse Response loading | `hound` (WAV), `symphonia` (multi-format) |
| Audio buffer | `dasp` |

---

## 5. Usage as a Claude Code Prompt

To use this document as a development prompt for Claude Code:

```
You are building the Audio Effects Engine for OpenGG, an open-source Linux gaming
companion app (Tauri 2 + Rust + Vue.js/Svelte). Refer to the complete effects
specification in this document. Each effect should be implemented as a modular
Rust plugin following the AudioEffect trait. The UI should provide real-time
controls with VU metering. Start by implementing [SPECIFIC EFFECT] with its
full parameter set as defined in section 2.X.
```

Replace `[SPECIFIC EFFECT]` with whichever effect you want to implement first. Recommended implementation order:

1. **EQ** — most impactful, most requested
2. **Compressor** — essential for mic processing
3. **Noise Gate** — gaming essential
4. **Limiter** — safety net, always needed
5. **RNNoise** — killer feature for voice chat
6. **Auto Gain** — quality of life
7. **Everything else** — in order of user demand
