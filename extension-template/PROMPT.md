# Build an OpenGG Extension with AI — Ready-to-Paste Prompt

You don't need to be a developer to create an OpenGG extension. Copy the prompt
below into any capable AI assistant (Claude, ChatGPT, etc.), replace the
`>>> DESCRIBE YOUR EXTENSION HERE <<<` line with what you want, and paste the
full contents of `AGENTS.md` where indicated. The AI will produce all the files.

Then: drop the resulting folder into `~/.local/share/opengg/extensions/`, run
`npm install && npm run build` inside it (only if it has a UI part), and open
**OpenGG → Settings → Extensions → Refresh**.

---

```
You are building an extension for OpenGG, a Linux gaming hub. Follow the
authoring contract below EXACTLY. Output every file with its full path and
complete contents (manifest.json, vite.config.js, package.json, src/index.ts,
src/Settings.vue if there's a UI, bin/<script> if there's a background part,
locales/en.json, assets/icon.svg). Do not invent APIs — use only what the
contract allows (the window.opengg invoke whitelist, window.Vue, and the daemon
executable model). After the files, give me a one-paragraph install summary.

The extension I want:
>>> DESCRIBE YOUR EXTENSION HERE <<<
(Examples: "a settings panel that lists my recent clips and shows how many I
recorded today" — UI only. "a background process that pauses my media player
when a game launches" — daemon only. "both: a panel to toggle a background
brightness routine".)

=== BEGIN OPENGG EXTENSION AUTHORING CONTRACT (AGENTS.md) ===
>>> PASTE THE FULL CONTENTS OF AGENTS.md HERE <<<
=== END OPENGG EXTENSION AUTHORING CONTRACT ===
```

---

## Tips for a good result
- Be specific about what the extension should show or do, and which of the five
  allowed read-only commands it needs (`get_clip_list`, `get_audio_devices`,
  `get_recorder_status`, `scan_extensions`, `list_user_locales`).
- For background behavior, describe the *event* that should trigger it (a sink
  appearing, a process launching) so the AI writes an efficient event-driven
  loop rather than a polling busy-loop.
- If something doesn't load, open Settings → Extensions, and check that the
  manifest `id` matches the `window.__ext_<id>` global (dashes → underscores).
