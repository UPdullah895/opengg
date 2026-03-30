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
