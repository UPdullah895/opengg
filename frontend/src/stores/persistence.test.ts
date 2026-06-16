import { describe, it, expect } from 'vitest'
import { deepMerge, runMigrations } from './persistence'

describe('runMigrations', () => {
  it('migrates clipsFolder to clip_directories', () => {
    const state = { settings: { clipsFolder: '/old/path' }, _schemaVersion: 0 } as any
    runMigrations(state)
    expect(state.settings.clip_directories).toEqual(['/old/path'])
    expect(state.settings.clipsFolder).toBeUndefined()
  })

  it('migrates screenshotDir to screenshotDirs', () => {
    const state = { settings: { screenshotDir: '/old/pic' }, _schemaVersion: 0 } as any
    runMigrations(state)
    expect(state.settings.screenshotDirs).toEqual(['/old/pic'])
    expect(state.settings.screenshotDir).toBeUndefined()
  })

  it('resets stale gsrMonitorTarget values', () => {
    const state = { settings: { gsrMonitorTarget: '1920x1080' }, _schemaVersion: 0 }
    runMigrations(state)
    expect(state.settings.gsrMonitorTarget).toBe('screen')
  })

  it('normalizes "connector|resolution" composite to the bare connector', () => {
    const state = { settings: { gsrMonitorTarget: 'DP-1|1920x1080' }, _schemaVersion: 9 }
    runMigrations(state)
    expect(state.settings.gsrMonitorTarget).toBe('DP-1')
  })

  it('resets a composite whose connector half is itself invalid', () => {
    const state = { settings: { gsrMonitorTarget: '1920x1080|DP-1' }, _schemaVersion: 9 }
    runMigrations(state)
    expect(state.settings.gsrMonitorTarget).toBe('screen')
  })

  it('migrates legacy captureTracks source labels to real monitor node names', () => {
    const state = {
      settings: { captureTracks: [
        { name: 'Track 1', source: 'Game' },
        { name: 'Track 2', source: 'Mic' },
        { name: 'Track 3', source: 'OpenGG_Aux.monitor' }, // already a real name — untouched
      ] },
      _schemaVersion: 10,
    } as any
    runMigrations(state)
    expect(state.settings.captureTracks.map((t: any) => t.source)).toEqual([
      'OpenGG_Game.monitor', 'OpenGG_Mic.monitor', 'OpenGG_Aux.monitor',
    ])
  })

  it('sets schema version after running', () => {
    const state = { settings: {}, _schemaVersion: 0 }
    runMigrations(state)
    expect(state._schemaVersion).toBeGreaterThan(0)
  })

  it('skips already-applied migrations', () => {
    const state = { settings: { clipsFolder: '/old/path' }, _schemaVersion: 8 }
    runMigrations(state)
    // Should remain untouched because schema is already at current version
    expect(state.settings.clipsFolder).toBe('/old/path')
  })
})

describe('deepMerge', () => {
  it('merges nested objects', () => {
    const a = { mixer: { volumes: { Game: 100 } } }
    const b = { mixer: { volumes: { Chat: 50 } } }
    const result = deepMerge(a, b)
    expect(result.mixer.volumes).toEqual({ Game: 100, Chat: 50 })
  })

  it('overrides primitive values', () => {
    const a = { name: 'old' }
    const b = { name: 'new' }
    expect(deepMerge(a, b)).toEqual({ name: 'new' })
  })

  it('replaces arrays (not merges them)', () => {
    const a = { items: [1, 2] }
    const b = { items: [3] }
    expect(deepMerge(a, b)).toEqual({ items: [3] })
  })

  it('returns a when b is null', () => {
    const a = { key: 'value' }
    expect(deepMerge(a, null)).toEqual(a)
  })

  it('returns b when a is not an object', () => {
    expect(deepMerge('string', { key: 'value' })).toEqual({ key: 'value' })
  })

  it('handles deeply nested structures', () => {
    const a = { a: { b: { c: 1 } } }
    const b = { a: { b: { d: 2 } } }
    const result = deepMerge(a, b)
    expect(result).toEqual({ a: { b: { c: 1, d: 2 } } })
  })
})
