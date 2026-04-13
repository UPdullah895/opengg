/**
 * TikTok Vertical Export — OpenGG Official Extension
 *
 * Adds a 9:16 export button to the Advanced Editor toolbar.
 * The button is rendered by the core editor when this extension is enabled.
 *
 * This extension has no settings panel — disabling it hides the 9:16 button.
 * Full export implementation (FFmpeg crop + pad pipeline) is roadmapped.
 */
(function () {
  window.__ext_tiktok_export = {
    settingsComponent: null,
  };
})();
