/**
 * Shared AudioContext singleton + one-time global unlock.
 *
 * Why a singleton?
 *   The browser limits the number of AudioContext instances. Sharing one across
 *   AdvancedEditor, ClipEditor, and any future component avoids the "too many
 *   AudioContext objects" warning and keeps the resume state consistent.
 *
 * Why a global pointerdown listener?
 *   The Autoplay Policy requires a "user activation" — a trusted event like a
 *   click or keydown — before the AudioContext can transition from 'suspended'
 *   to 'running'. Calling resume() inside onPlay/togglePlay is often too late
 *   (the event dispatch chain may no longer count as a user activation).
 *   Capturing the very first pointerdown on window, in the capture phase, is
 *   the most reliable place: it fires before any Vue handler and is always
 *   considered a trusted gesture.
 */

let _ctx: AudioContext | null = null

/**
 * Map a recorded audio-track title to a friendly label for display.
 * New recordings already carry friendly titles (set at save time), but older files have
 * the raw capture-source node name (e.g. "OpenGG_Game.monitor"). Mirrors the backend
 * `friendly_track_name` in commands.rs. Returns null when no mapping applies so callers
 * keep their own fallback (e.g. "Audio 1").
 */
export function friendlyTrackName(title: string | undefined | null): string | null {
  if (!title) return null
  const t = title.startsWith('device:') ? title.slice('device:'.length) : title
  const m = t.match(/^OpenGG_(.+)\.monitor$/)
  if (m) return m[1] // Game / Chat / Media / Aux / Mic
  if (t === 'default_output') return 'Desktop'
  if (t === 'default_input') return 'Mic'
  if (t.startsWith('alsa_input.')) return 'Mic'
  if (t.endsWith('.monitor')) return 'Output'
  return null
}

export function getAudioContext(): AudioContext {
  if (!_ctx || _ctx.state === 'closed') _ctx = new AudioContext()
  return _ctx
}

export async function resumeAudioContext(): Promise<void> {
  try {
    const ctx = getAudioContext()
    if (ctx.state === 'suspended') await ctx.resume()
  } catch (e) {
    console.warn('[audio] resume failed:', e)
  }
}

let _unlockInstalled = false

/**
 * Call once (from App.vue onMounted).
 * Installs a capture-phase pointerdown listener on window that resumes the
 * AudioContext on the user's very first tap/click anywhere in the app, then
 * removes itself.
 */
export function installAudioUnlocker(): void {
  if (_unlockInstalled) return
  _unlockInstalled = true

  const unlock = async () => {
    await resumeAudioContext()
    // Remove as soon as the context is running; keep retrying if it isn't
    // (e.g. browser denied the first attempt).
    if (getAudioContext().state !== 'suspended') {
      window.removeEventListener('pointerdown', unlock, true)
    }
  }

  // capture: true — fires before any bubbling handler
  window.addEventListener('pointerdown', unlock, true)
}
