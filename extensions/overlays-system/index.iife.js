/**
 * Overlays System — OpenGG Official Extension
 *
 * Enables the O1 (Overlays) track in the Advanced Editor timeline.
 * Users can drag text and image overlays onto the video preview.
 *
 * This extension has no settings panel — it activates the built-in
 * overlay rendering that is already part of the core editor. Disabling
 * this extension hides the Overlays tab and removes the O1 timeline track.
 */
(function () {
  // No settingsComponent — hasSettings is false in manifest.json.
  // The extension just signals that the overlays feature is active.
  window.__ext_overlays_system = {
    settingsComponent: null,
  };
})();
