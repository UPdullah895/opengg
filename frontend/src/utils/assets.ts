/**
 * Local Asset URL utility — maps absolute paths to HTTP URLs.
 *
 * URL scheme:
 *   http://127.0.0.1:PORT/media/home/user/Videos/clip.mp4
 *   → warp::fs::dir("/") serves /home/user/Videos/clip.mp4
 *
 * warp::fs handles Range requests (206), streaming, and Content-Type
 * automatically — no manual byte slicing needed.
 */

import { invoke } from '@tauri-apps/api/core'

let _port: number | null = null
let _token: string | null = null

export async function getMediaPort(): Promise<number> {
  if (_port !== null) return _port
  const MAX_ATTEMPTS = 5
  const DELAY_MS = 500
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    try {
      _port = await invoke<number>('get_media_server_port')
      return _port
    } catch {
      if (attempt < MAX_ATTEMPTS - 1) {
        await new Promise<void>(r => setTimeout(r, DELAY_MS))
      }
    }
  }
  console.error('media port: could not connect after 5 attempts')
  _port = 0
  return _port
}

export async function getMediaToken(): Promise<string> {
  if (_token !== null) return _token
  const MAX_ATTEMPTS = 5
  const DELAY_MS = 500
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    try {
      _token = await invoke<string>('get_media_server_token')
      return _token
    } catch {
      if (attempt < MAX_ATTEMPTS - 1) {
        await new Promise<void>(r => setTimeout(r, DELAY_MS))
      }
    }
  }
  console.error('media token: could not connect after 5 attempts')
  _token = ''
  return _token
}

/**
 * Convert an absolute file path to an HTTP URL served by our warp server.
 *
 * Example:
 *   mediaUrl('/home/user/Videos/clip.mp4', 33955, 'token123')
 *   → 'http://127.0.0.1:33955/media/home/user/Videos/clip.mp4?token=token123'
 */
export function mediaUrl(absolutePath: string, port: number, token: string): string {
  if (!absolutePath || !port || !token) return ''
  // Ensure the path starts with / (absolute)
  const cleanPath = absolutePath.startsWith('/') ? absolutePath : `/${absolutePath}`
  // Use 127.0.0.1, not "localhost": the media server binds IPv4-only (127.0.0.1),
  // but "localhost" can resolve to IPv6 ::1 first on many systems, giving
  // "Could not connect to localhost: Connection refused".
  return `http://127.0.0.1:${port}/media${cleanPath}?token=${encodeURIComponent(token)}`
}

export async function mediaUrlAsync(absolutePath: string): Promise<string> {
  return mediaUrl(absolutePath, await getMediaPort(), await getMediaToken())
}
