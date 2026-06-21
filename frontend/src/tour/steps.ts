import type { Placement } from '@floating-ui/vue'
import { useAudioStore } from '../stores/audio'

export type TourPage = 'home' | 'mixer' | 'clips' | 'devices' | 'settings'

export interface TourStep {
  /** Stable id; i18n copy lives under `tour.steps.<id>.{title,body,action}`. */
  id: string
  /** Page the tour routes to before showing this step. */
  page: TourPage
  /** CSS selector for the highlighted element. Omit for a centered, no-spotlight step. */
  target?: string
  /** Preferred side of the target for the card (LTR intent; mirrored in RTL at runtime). */
  placement?: Placement
  /** Optional interactive suggestion: when `detect()` becomes true the tour auto-advances. */
  action?: { detect: () => boolean }
}

/**
 * The guided-tour script. This is the single place to review/edit step order and which
 * real element each step points at. Copy is in `locales/{en,ar}.json` under `tour.steps`.
 */
export const TOUR_STEPS: TourStep[] = [
  { id: 'welcome', page: 'home' },
  { id: 'nav', page: 'home', target: '[data-tour="nav-mixer"]', placement: 'right' },
  { id: 'dashboard', page: 'home', target: '[data-tour="home-dashboard"]', placement: 'bottom' },
  { id: 'mixerIntro', page: 'mixer', target: '[data-tour="mixer-channels"]', placement: 'bottom' },
  {
    id: 'mixerAction',
    page: 'mixer',
    target: '[data-tour="mixer-channels"]',
    placement: 'top',
    // "Audio split" = an app routed to a channel. Auto-advances once any app has a channel.
    action: { detect: () => useAudioStore().allApps.some(a => !!a.channel) },
  },
  { id: 'clips', page: 'clips', target: '[data-tour="clips-grid"]', placement: 'top' },
  { id: 'recorder', page: 'home', target: '[data-tour="home-recorder"]', placement: 'bottom' },
  { id: 'devices', page: 'devices', target: '[data-tour="devices-list"]', placement: 'top' },
  { id: 'settings', page: 'settings', target: '[data-tour="nav-settings"]', placement: 'right' },
  { id: 'finish', page: 'settings', target: '[data-tour="settings-replay"]', placement: 'top' },
]
