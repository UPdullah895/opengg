/**
 * Theme System — loads theme.json → applies CSS variables to :root.
 *
 * Theme file: ~/.config/opengg/theme.json
 * Structure:
 *   {
 *     "colors": { "--accent": "#E94560", "--bg-surface": "#0f1117", ... },
 *     "layout": { "--clips-grid-cols": "4", "--radius": "6px", ... }
 *   }
 */

import { invoke } from '@tauri-apps/api/core'

export interface Theme {
  colors: Record<string, string>
  layout: Record<string, string>
}

let _currentTheme: Theme | null = null

/** Load theme from disk and apply to DOM */
export async function loadTheme(): Promise<Theme> {
  try {
    const json = await invoke<string>('load_theme')
    const theme = JSON.parse(json) as Theme
    applyTheme(theme)
    _currentTheme = theme
    return theme
  } catch (e) {
    console.warn('Failed to load theme:', e)
    return { colors: {}, layout: {} }
  }
}

/** Save current theme to disk */
export async function saveTheme(theme: Theme): Promise<void> {
  try {
    await invoke('save_theme', { themeJson: JSON.stringify(theme, null, 2) })
    applyTheme(theme)
    _currentTheme = theme
  } catch (e) {
    console.error('Failed to save theme:', e)
  }
}

/** Apply a theme object to the document :root element */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement

  if (theme.colors) {
    for (const [key, value] of Object.entries(theme.colors)) {
      root.style.setProperty(key, value)
    }
  }

  if (theme.layout) {
    for (const [key, value] of Object.entries(theme.layout)) {
      root.style.setProperty(key, value)
    }
  }
}

/** Get current loaded theme */
export function getCurrentTheme(): Theme | null {
  return _currentTheme
}

/** Reset theme to defaults by removing custom properties */
export async function resetTheme(): Promise<void> {
  const root = document.documentElement
  if (_currentTheme) {
    for (const key of [...Object.keys(_currentTheme.colors), ...Object.keys(_currentTheme.layout)]) {
      root.style.removeProperty(key)
    }
  }
  // Load defaults from backend
  await loadTheme()
}
