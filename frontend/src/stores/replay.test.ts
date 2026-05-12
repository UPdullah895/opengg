import { describe, it, expect } from 'vitest'
import { normalizeGameTitle, parseSearchQuery, buildSearch } from './replay'
import type { DateFormat } from './replay'

describe('normalizeGameTitle', () => {
  it('lowercases and trims', () => {
    expect(normalizeGameTitle('  HELLO  ')).toBe('hello')
  })

  it('replaces special chars with spaces', () => {
    expect(normalizeGameTitle('Game: Name!')).toBe('game name')
  })

  it('removes diacritics', () => {
    expect(normalizeGameTitle('Café Racer')).toBe('cafe racer')
  })

  it('removes trademark symbols', () => {
    expect(normalizeGameTitle('Game™ ©®')).toBe('game')
  })

  it('collapses multiple spaces', () => {
    expect(normalizeGameTitle('a    b')).toBe('a b')
  })

  it('handles empty string', () => {
    expect(normalizeGameTitle('')).toBe('')
  })
})

describe('parseSearchQuery', () => {
  it('returns empty array for empty query', () => {
    expect(parseSearchQuery('', 'YMD')).toEqual([])
  })

  it('returns empty array for whitespace-only query', () => {
    expect(parseSearchQuery('   ', 'YMD')).toEqual([])
  })

  it('parses plain tokens', () => {
    expect(parseSearchQuery('hello world', 'YMD')).toEqual(['hello', 'world'])
  })

  it('parses month names', () => {
    expect(parseSearchQuery('jan', 'YMD')).toEqual(['__m01'])
    expect(parseSearchQuery('december', 'YMD')).toEqual(['__m12'])
  })

  it('parses YYYY/MM/DD in YMD mode', () => {
    expect(parseSearchQuery('2024/03/15', 'YMD')).toEqual(['2024/03/15'])
  })

  it('parses YYYY/DD/MM in YDM mode', () => {
    expect(parseSearchQuery('2024/15/03', 'YDM')).toEqual(['2024/03/15'])
  })

  it('parses YYYY-MM-DD', () => {
    expect(parseSearchQuery('2024-03-15', 'YMD')).toEqual(['2024/03/15'])
  })

  it('parses year-month', () => {
    expect(parseSearchQuery('2024/3', 'YMD')).toEqual(['2024/03'])
  })

  it('parses four-digit year', () => {
    expect(parseSearchQuery('2024', 'YMD')).toEqual(['2024'])
  })
})

describe('buildSearch', () => {
  it('combines name, filename, game, and date parts', () => {
    const result = buildSearch({
      custom_name: 'My Clip',
      filename: 'clip.mp4',
      game: 'Apex',
      created: '2024-03-15T10:00:00',
    })
    expect(result).toContain('my clip')
    expect(result).toContain('clip.mp4')
    expect(result).toContain('apex')
    expect(result).toContain('2024')
    expect(result).toContain('__m03')
    expect(result).toContain('2024/03/15')
  })

  it('handles missing fields', () => {
    const result = buildSearch({
      custom_name: '',
      filename: '',
      game: '',
      created: '',
    })
    expect(result).toBe('')
  })
})
