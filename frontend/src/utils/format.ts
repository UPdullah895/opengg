export function fmtDur(s: number): string {
  if (!s) return '0:00'
  return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`
}

export function fmtSize(b: number): string {
  if (!b) return '0 B'
  const u = ['B', 'KB', 'MB', 'GB']
  let i = 0, v = b
  while (v >= 1024 && i < 3) { v /= 1024; i++ }
  return `${v.toFixed(i ? 1 : 0)} ${u[i]}`
}

export function fmtRes(w: number, h: number): string {
  if (!w) return ''
  if (h >= 2160) return '4K'
  if (h >= 1440) return '1440p'
  if (h >= 1080) return '1080p'
  if (h >= 720) return '720p'
  return `${w}×${h}`
}

export function fmtDate(created: string): string {
  if (!created) return ''
  const d = new Date(created.replace(' ', 'T'))
  if (isNaN(d.getTime())) return ''
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
}
