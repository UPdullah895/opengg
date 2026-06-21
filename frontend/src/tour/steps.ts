import type { Placement } from '@floating-ui/vue'
import { useAudioStore } from '../stores/audio'
import { useReplayStore } from '../stores/replay'

export type TourPage = 'home' | 'mixer' | 'clips' | 'devices' | 'settings'

/** State-aware call-to-action a step can surface in its card. */
export type TourCta = 'audioSetup' | 'recorderInstall'

export interface TourStep {
  /** Stable id; i18n copy lives under `tour.steps.<id>.{title,body,action,deep}`. */
  id: string
  /** Page the tour routes to before showing this step. */
  page: TourPage
  /** CSS selector for the highlighted element. Omit for a centered, no-spotlight step. */
  target?: string
  /** Preferred side of the target for the card (LTR intent; mirrored in RTL at runtime). */
  placement?: Placement
  /** Optional interactive suggestion: when `detect()` is true the card shows a ✓ (no auto-advance). */
  action?: { detect: () => boolean }
  /** True when `tour.steps.<id>.deep` exists → the card shows a "More" expander. */
  deepKey?: boolean
  /** State-aware CTA rendered in the card (e.g. set up audio engine / install recorder). */
  cta?: TourCta
  /** When present and false, the step is skipped (used for the editor steps). */
  condition?: () => boolean
}

/** A real, openable clip exists (excludes skeletons and the in-memory tour demo card). */
const hasRealClip = () =>
  useReplayStore().clips.some((c: { isSkeleton?: boolean; isDemo?: boolean }) => !c.isSkeleton && !c.isDemo)

/**
 * The guided-tour script. Steps are grouped by page and visited in sidebar order
 * (Home → Mixer → Clips → editor → Devices → Settings) with no page revisited.
 * Copy is in `locales/{en,ar}.json` under `tour.steps`.
 */
export const TOUR_STEPS: TourStep[] = [
  // ── Home ──
  { id: 'welcome', page: 'home', deepKey: true },
  { id: 'nav', page: 'home', target: '[data-tour="nav-mixer"]', placement: 'right', deepKey: true },
  { id: 'dashboard', page: 'home', target: '[data-tour="home-dashboard"]', placement: 'bottom', deepKey: true },
  // Recorder install is user-driven; the card surfaces a distro-aware install helper when missing.
  { id: 'recorder', page: 'home', target: '[data-tour="home-recorder"]', placement: 'bottom', deepKey: true, cta: 'recorderInstall' },

  // ── Mixer ──
  { id: 'mixerIntro', page: 'mixer', target: '[data-tour="mixer-channels"]', placement: 'bottom', deepKey: true },
  {
    id: 'mixerAction',
    page: 'mixer',
    target: '[data-tour="mixer-channels"]',
    placement: 'top',
    deepKey: true,
    // If the engine isn't set up, the card shows a "set up audio engine" CTA instead of the
    // split suggestion (handled in GuidedTour). "Audio split" = an app routed to a channel.
    cta: 'audioSetup',
    action: { detect: () => useAudioStore().allApps.some(a => !!a.channel) },
  },

  // ── Clips (+ conditional editor) ──
  { id: 'clips', page: 'clips', target: '[data-tour="clips-grid"]', placement: 'top', deepKey: true },
  { id: 'editorTracks', page: 'clips', target: '[data-tour="editor-timeline"]', placement: 'top', deepKey: true, condition: hasRealClip },
  { id: 'editorFilters', page: 'clips', target: '[data-tour="editor-filters"]', placement: 'left', deepKey: true, condition: hasRealClip },

  // ── Devices ──
  { id: 'devices', page: 'devices', target: '[data-tour="devices-list"]', placement: 'top', deepKey: true },

  // ── Settings ──
  { id: 'settings', page: 'settings', target: '[data-tour="nav-settings"]', placement: 'right', deepKey: true },
  { id: 'finish', page: 'settings', target: '[data-tour="settings-replay"]', placement: 'top' },
]
