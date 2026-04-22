/**
 * Device image & icon registry for OpenGG.
 *
 * Drop PNG files named `{vid}_{pid}.png` into `src/assets/devices/`.
 * They are auto-discovered at build time via Vite's import.meta.glob.
 * Use `scripts/fetch_device_assets.sh` to populate the folder.
 */

const _deviceImages = import.meta.glob('./devices/*.png', { eager: true, import: 'default' }) as Record<string, string>
const _deviceImagesLower = Object.fromEntries(
  Object.entries(_deviceImages).map(([k, v]) => [k.toLowerCase(), v])
)

const PLACEHOLDERS: Record<string, string> = {
  headset: `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='300' height='200' viewBox='0 0 300 200'%3E%3Crect width='300' height='200' fill='%23171923'/%3E%3Cpath d='M60 140v-30a60 60 0 01120 0v30' stroke='%23E94560' stroke-width='8' fill='none' stroke-linecap='round'/%3E%3Crect x='40' y='130' width='30' height='50' rx='10' fill='%23E94560'/%3E%3Crect x='230' y='130' width='30' height='50' rx='10' fill='%23E94560'/%3E%3C/svg%3E`,
  mouse:   `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='300' height='200' viewBox='0 0 300 200'%3E%3Cellipse cx='150' cy='100' rx='55' ry='75' fill='none' stroke='%23E94560' stroke-width='5'/%3E%3Cline x1='150' y1='55' x2='150' y2='115' stroke='%23E94560' stroke-width='3' stroke-linecap='round'/%3E%3Crect x='146' y='45' width='8' height='18' rx='4' fill='%23E94560'/%3E%3Cline x1='100' y1='75' x2='130' y2='75' stroke='%23E94560' stroke-width='2' stroke-linecap='round' opacity='.5'/%3E%3Cline x1='170' y1='75' x2='200' y2='75' stroke='%23E94560' stroke-width='2' stroke-linecap='round' opacity='.5'/%3E%3C/svg%3E`,
  default: `data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='300' height='200' viewBox='0 0 300 200'%3E%3Crect width='300' height='200' fill='%23171923'/%3E%3Ccircle cx='150' cy='100' r='60' fill='none' stroke='%23E94560' stroke-width='8' stroke-dasharray='20 10'/%3E%3C/svg%3E`,
}

/** Returns the PNG path for a device (auto-discovered from src/assets/devices/), or a type-appropriate SVG placeholder. */
export function getDeviceImage(vid: number, pid: number, type = 'default'): string {
  const key = `${vid.toString(16).padStart(4, '0')}_${pid.toString(16).padStart(4, '0')}`
  const img = _deviceImagesLower[`./devices/${key}.png`]
  if (img) return img
  return PLACEHOLDERS[type] ?? PLACEHOLDERS.default
}

const TRIM_WORDS = /\b(wireless|wired|gaming|mouse|keyboard|headset|headphones?|optical|rgb|ultra|superlight|lightspeed|pro\s+x?|x\s+plus)\b/gi
const TRIM_MULTI_SPACE = /\s{2,}/g

/** Strips common marketing words from device names for compact display. */
export function trimDeviceName(name: string): string {
  return name
    .replace(TRIM_WORDS, '')
    .replace(TRIM_MULTI_SPACE, ' ')
    .trim()
}

// ── SVG icons ─────────────────────────────────────────────────────────────────

/** Standard gear / cog icon (24×24) */
export const ICON_GEAR = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>`

/** Grid-view icon (4 squares, 24×24) */
export const ICON_GRID = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>`

/** List-view icon (3 horizontal lines, 24×24) */
export const ICON_LIST = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg>`

/** Battery icon (24×24, no fill — monochromatic) */
export const ICON_BATTERY = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="18" height="11" rx="2"/><path d="M22 11v3"/></svg>`

/** Charging bolt (24×24, monochromatic) */
export const ICON_BOLT = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>`

/** SideTone / audio icon (24×24) */
export const ICON_AUDIO = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>`

/** Power / auto-off icon (24×24) */
export const ICON_POWER = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18.36 6.64A9 9 0 0 1 20.77 15"/><path d="M6.16 6.16a9 9 0 1 0 12.68 12.68"/><line x1="12" y1="2" x2="12" y2="12"/></svg>`

/** Volume / limiter icon (24×24) */
export const ICON_VOLUME = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>`

/** Bluetooth icon (24×24) */
export const ICON_BLUETOOTH = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="6.5 6.5 17.5 17.5 12 23 12 1 17.5 6.5 6.5 17.5"/></svg>`

/** Volume-lower icon for call volume (24×24) */
export const ICON_VOLUME_DOWN = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>`

/** Delete / trash icon (24×24) */
export const ICON_DELETE = `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/></svg>`
