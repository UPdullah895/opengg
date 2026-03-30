import { createI18n } from 'vue-i18n'
import { reactive } from 'vue'

// ── Build-time locale discovery ──────────────────────────────────────────────
// import.meta.glob scans src/locales/*.json at build time. All bundled locale
// files are loaded eagerly so they're available synchronously.
// Each JSON MAY contain a top-level "_meta" object:
//   { "name": "Español", "dir": "ltr" }
// If absent, the display name falls back to capitalising the filename code.
type LocaleData = { _meta?: { name?: string; dir?: 'ltr' | 'rtl' }; [key: string]: unknown }

const bundledModules = import.meta.glob<{ default: LocaleData }>('./locales/*.json', { eager: true })

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const messages: Record<string, any> = {}
const detected: Array<{ code: string; label: string; dir: 'ltr' | 'rtl' }> = []

for (const [path, mod] of Object.entries(bundledModules)) {
  // './locales/en.json'  →  'en'
  const code = path.slice('./locales/'.length, -'.json'.length)
  const data = mod.default
  messages[code] = data
  const meta = data._meta ?? {}
  detected.push({
    code,
    label: meta.name ?? (code.charAt(0).toUpperCase() + code.slice(1)),
    dir:   meta.dir  ?? 'ltr',
  })
}

// ── i18n instance ────────────────────────────────────────────────────────────
export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages,
})

// ── Reactive LANGUAGES list ──────────────────────────────────────────────────
// reactive() — not ref() — so callers can use LANGUAGES.find() / v-for directly
// without .value unwrapping, while still tracking additions at runtime.
export const LANGUAGES = reactive(detected)

// ── Runtime locale registration (user-dropped files) ────────────────────────
/**
 * Register a locale that was not bundled at build time (e.g., a file the user
 * dropped into ~/.config/opengg/locales/).
 * Safe to call multiple times with the same code — idempotent.
 */
export function registerLocale(
  code: string,
  data: Record<string, unknown>,
  name: string,
  dir: 'ltr' | 'rtl',
): void {
  if (LANGUAGES.find(l => l.code === code)) return // already registered
  i18n.global.setLocaleMessage(code, data)
  LANGUAGES.push({ code, label: name, dir })
}
