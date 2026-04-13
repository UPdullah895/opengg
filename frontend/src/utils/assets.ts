/**
 * Local Asset URL utility — maps absolute paths to HTTP URLs.
 *
 * URL scheme:
 *   http://localhost:PORT/media/home/user/Videos/clip.mp4
 *   → warp::fs::dir("/") serves /home/user/Videos/clip.mp4
 *
 * warp::fs handles Range requests (206), streaming, and Content-Type
 * automatically — no manual byte slicing needed.
 */

import { invoke } from '@tauri-apps/api/core'

let _port: number | null = null

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

/**
 * Convert an absolute file path to an HTTP URL served by our warp server.
 *
 * Example:
 *   mediaUrl('/home/user/Videos/clip.mp4', 33955)
 *   → 'http://localhost:33955/media/home/user/Videos/clip.mp4'
 */
export function mediaUrl(absolutePath: string, port: number): string {
  if (!absolutePath || !port) return ''
  // Ensure the path starts with / (absolute)
  const cleanPath = absolutePath.startsWith('/') ? absolutePath : `/${absolutePath}`
  return `http://localhost:${port}/media${cleanPath}`
}

export async function mediaUrlAsync(absolutePath: string): Promise<string> {
  return mediaUrl(absolutePath, await getMediaPort())
}
