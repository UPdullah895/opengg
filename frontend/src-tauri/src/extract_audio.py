#!/usr/bin/env python3
import re

# Read original file
with open("commands.rs", "r") as f:
    lines = f.readlines()

# Define inclusive 1-indexed ranges to extract to audio.rs
# These must be sorted and non-overlapping
extract_ranges = [
    (23, 876),      # First audio block
    (3388, 3521),   # Audio Devices
    (3523, 3696),   # VU meters
    (4654, 4792),   # Virtual Audio + hydrate
    (4882, 4922),   # list_audio_sinks + get_session_type
    (6474, 6559),   # Tests
]

# Build audio.rs content
audio_lines = []
audio_lines.append("//! Audio commands and helpers for OpenGG.\n")
audio_lines.append("\n")
audio_lines.append("use serde::{Deserialize, Serialize};\n")
audio_lines.append("use std::collections::HashMap;\n")
audio_lines.append("use std::process::Command;\n")
audio_lines.append("use std::sync::atomic::Ordering;\n")
audio_lines.append("use std::sync::{Arc, Mutex};\n")
audio_lines.append("use tauri::{command, AppHandle, Emitter, Manager};\n")
audio_lines.append("use super::{AU_PATH, AU_IFACE, call_dbus, call_dbus_void, run_cmd};\n")
audio_lines.append("\n")

for start, end in extract_ranges:
    for i in range(start - 1, end):
        audio_lines.append(lines[i])

# Build commands.rs content
# We'll iterate through lines and skip the extract ranges
keep = [True] * len(lines)
for start, end in extract_ranges:
    for i in range(start - 1, end):
        keep[i] = False

commands_lines = []
for i, line in enumerate(lines):
    if keep[i]:
        commands_lines.append(line)

# Now apply modifications to commands_lines
modified = []
for line in commands_lines:
    # Make D-Bus constants pub(crate)
    if line.startswith("const AU_PATH:"):
        modified.append("pub(crate) " + line)
    elif line.startswith("const AU_IFACE:"):
        modified.append("pub(crate) " + line)
    elif line.startswith("const RP_PATH:"):
        modified.append("pub(crate) " + line)
    elif line.startswith("const RP_IFACE:"):
        modified.append("pub(crate) " + line)
    elif line.startswith("const DV_PATH:"):
        modified.append("pub(crate) " + line)
    elif line.startswith("const DV_IFACE:"):
        modified.append("pub(crate) " + line)
    # Make shared helpers pub(crate)
    elif line.startswith("fn run_cmd("):
        modified.append("pub(crate) " + line)
    elif line.startswith("async fn call_dbus<"):
        modified.append("pub(crate) " + line)
    elif line.startswith("async fn call_dbus_void("):
        modified.append("pub(crate) " + line)
    else:
        modified.append(line)

# Insert mod audio; pub use audio::*; after the pub(crate) const lines
# Find the line after const DV_IFACE
insert_after = None
for idx, line in enumerate(modified):
    if line.startswith("pub(crate) const DV_IFACE:"):
        insert_after = idx
        break

if insert_after is not None:
    # Insert after this line
    modified.insert(insert_after + 1, "\n")
    modified.insert(insert_after + 2, "mod audio;\n")
    modified.insert(insert_after + 3, "pub use audio::*;\n")

# Write files
with open("commands/audio.rs", "w") as f:
    f.writelines(audio_lines)

with open("commands.rs", "w") as f:
    f.writelines(modified)

print("Done!")
