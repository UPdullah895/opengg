import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface DependencyStatus {
  binary: string
  available: boolean
  feature: string
}

export interface DeviceAccessStatus {
  ratbagd_available: boolean
  in_input_group: boolean
  in_audio_group: boolean
  in_video_group: boolean
  udev_rules_present: boolean
}

export interface DistroInfo {
  id: string
  id_like: string
}

export interface PackageMap {
  arch: string
  debian: string
  fedora: string
  note?: string
}

export interface AccessFixMap {
  commands: string[]
  note: string
}

// Package installation commands per binary and distro
export const PACKAGE_MAPS: Record<string, PackageMap> = {
  'gpu-screen-recorder': {
    arch: 'pacman -S gpu-screen-recorder',
    debian: 'flatpak install flathub com.dec05eba.gpu_screen_recorder',
    fedora: 'flatpak install flathub com.dec05eba.gpu_screen_recorder',
    note: 'installHint.gsrNote',
  },
  'ffmpeg': {
    arch: 'pacman -S ffmpeg',
    debian: 'apt install ffmpeg',
    fedora: 'dnf install ffmpeg',
    note: 'installHint.ffmpegNote',
  },
  'ffprobe': {
    arch: 'pacman -S ffmpeg',
    debian: 'apt install ffmpeg',
    fedora: 'dnf install ffmpeg',
    note: 'installHint.ffprobeNote',
  },
  'pactl': {
    arch: 'pacman -S libpulse',
    debian: 'apt install pulseaudio-utils',
    fedora: 'dnf install pulseaudio-utils',
  },
  'pw-link': {
    arch: 'pacman -S pipewire',
    debian: 'apt install pipewire',
    fedora: 'dnf install pipewire',
  },
  'jalv': {
    arch: 'pacman -S jalv',
    debian: 'apt install jalv',
    fedora: 'dnf install jalv',
  },
  'headsetcontrol': {
    arch: 'pacman -S headsetcontrol',
    debian: 'apt install headsetcontrol',
    fedora: 'dnf install headsetcontrol',
  },
  'xdotool': {
    arch: 'pacman -S xdotool',
    debian: 'apt install xdotool',
    fedora: 'dnf install xdotool',
  },
}

// Device access fixes per item and distro family
export const ACCESS_FIX_MAPS: Record<string, { arch?: AccessFixMap; debian?: AccessFixMap; fedora?: AccessFixMap; unknown?: AccessFixMap }> = {
  'ratbagd': {
    arch: {
      commands: ['sudo pacman -S libratbag', 'sudo systemctl enable --now ratbagd'],
      note: 'accessFixHint.ratbagdNote',
    },
    debian: {
      commands: ['sudo apt install ratbagd', 'sudo systemctl enable --now ratbagd'],
      note: 'accessFixHint.ratbagdNote',
    },
    fedora: {
      commands: ['sudo dnf install libratbag-ratbagd', 'sudo systemctl enable --now ratbagd'],
      note: 'accessFixHint.ratbagdNote',
    },
    unknown: {
      commands: [
        '# Arch: sudo pacman -S libratbag && sudo systemctl enable --now ratbagd',
        '# Debian/Ubuntu: sudo apt install ratbagd && sudo systemctl enable --now ratbagd',
        '# Fedora: sudo dnf install libratbag-ratbagd && sudo systemctl enable --now ratbagd',
      ],
      note: 'accessFixHint.ratbagdNote',
    },
  },
  'inputGroup': {
    arch: {
      commands: ['sudo usermod -aG input $USER'],
      note: 'accessFixHint.reloginNote',
    },
    debian: {
      commands: ['sudo usermod -aG input $USER'],
      note: 'accessFixHint.reloginNote',
    },
    fedora: {
      commands: ['sudo usermod -aG input $USER'],
      note: 'accessFixHint.reloginNote',
    },
    unknown: {
      commands: ['sudo usermod -aG input $USER'],
      note: 'accessFixHint.reloginNote',
    },
  },
  'audioGroup': {
    arch: {
      commands: ['sudo usermod -aG audio $USER'],
      note: 'accessFixHint.reloginNote',
    },
    debian: {
      commands: ['sudo usermod -aG audio $USER'],
      note: 'accessFixHint.reloginNote',
    },
    fedora: {
      commands: ['sudo usermod -aG audio $USER'],
      note: 'accessFixHint.reloginNote',
    },
    unknown: {
      commands: ['sudo usermod -aG audio $USER'],
      note: 'accessFixHint.reloginNote',
    },
  },
  'videoGroup': {
    arch: {
      commands: ['sudo usermod -aG video $USER'],
      note: 'accessFixHint.reloginNote',
    },
    debian: {
      commands: ['sudo usermod -aG video $USER'],
      note: 'accessFixHint.reloginNote',
    },
    fedora: {
      commands: ['sudo usermod -aG video $USER'],
      note: 'accessFixHint.reloginNote',
    },
    unknown: {
      commands: ['sudo usermod -aG video $USER'],
      note: 'accessFixHint.reloginNote',
    },
  },
  'udevRules': {
    arch: {
      commands: ['./dev.sh setup  # From the OpenGG repository root'],
      note: 'accessFixHint.udevNote',
    },
    debian: {
      commands: ['./dev.sh setup  # From the OpenGG repository root'],
      note: 'accessFixHint.udevNote',
    },
    fedora: {
      commands: ['./dev.sh setup  # From the OpenGG repository root'],
      note: 'accessFixHint.udevNote',
    },
    unknown: {
      commands: [
        './dev.sh setup  # From the OpenGG repository root',
        '# Or manually: sudo cp packaging/udev-rules/*.rules /etc/udev/rules.d/',
      ],
      note: 'accessFixHint.udevNote',
    },
  },
}

// Resolve distro family from ID and ID_LIKE
function resolveDistroFamily(id: string, idLike: string): 'arch' | 'debian' | 'fedora' | 'unknown' {
  const searchStr = `${id} ${idLike}`.toLowerCase()

  if (searchStr.includes('arch')) return 'arch'
  if (searchStr.includes('debian') || searchStr.includes('ubuntu')) return 'debian'
  if (searchStr.includes('fedora') || searchStr.includes('rhel') || searchStr.includes('centos')) return 'fedora'

  return 'unknown'
}

export function getInstallCommand(binary: string, distroId: string, distroIdLike: string): { command: string; note?: string } {
  const pkgMap = PACKAGE_MAPS[binary]
  if (!pkgMap) return { command: '' }

  const family = resolveDistroFamily(distroId, distroIdLike)

  if (family === 'unknown') {
    // Return all three commands formatted as alternatives
    const cmd = `# Try one of:\n${pkgMap.arch}\n${pkgMap.debian}\n${pkgMap.fedora}`
    return { command: cmd, note: pkgMap.note }
  }

  return {
    command: pkgMap[family],
    note: pkgMap.note,
  }
}

export function getAccessFixCommand(item: string, distroId: string, distroIdLike: string): { commands: string[]; note?: string } {
  const fixMap = ACCESS_FIX_MAPS[item]
  if (!fixMap) return { commands: [] }

  const family = resolveDistroFamily(distroId, distroIdLike)
  const fix = fixMap[family] || fixMap.unknown

  if (!fix) return { commands: [] }

  return {
    commands: fix.commands,
    note: fix.note,
  }
}

// Module-level singleton — fetched once, shared across all consumers.
const deps = ref<DependencyStatus[]>([])
const distroInfo = ref<DistroInfo>({ id: '', id_like: '' })
let _loaded = false
let _distroLoaded = false

export async function loadDependencyStatus() {
  if (_loaded) return
  try {
    deps.value = await invoke<DependencyStatus[]>('get_dependency_status')
  } catch (e) {
    console.error('Failed to load dependency status:', e)
    deps.value = []
  }
  _loaded = true
}

export async function loadDistroInfo() {
  if (_distroLoaded) return
  try {
    distroInfo.value = await invoke<DistroInfo>('get_distro_info')
  } catch (e) {
    console.error('Failed to load distro info:', e)
    distroInfo.value = { id: '', id_like: '' }
  }
  _distroLoaded = true
}

export function missing(feature: string): boolean {
  return deps.value.some(d => d.feature === feature && !d.available)
}

export function isAvailable(feature: string): boolean {
  return deps.value.some(d => d.feature === feature && d.available)
}

export function missingBinary(binary: string): boolean {
  const dep = deps.value.find(d => d.binary === binary)
  return dep ? !dep.available : true
}

// Device access status singleton
const deviceAccess = ref<DeviceAccessStatus>({
  ratbagd_available: false,
  in_input_group: false,
  in_audio_group: false,
  in_video_group: false,
  udev_rules_present: false,
})
let _deviceLoaded = false

export async function loadDeviceAccessStatus() {
  if (_deviceLoaded) return
  try {
    deviceAccess.value = await invoke<DeviceAccessStatus>('get_device_access_status')
  } catch (e) {
    console.error('Failed to load device access status:', e)
    deviceAccess.value = {
      ratbagd_available: false,
      in_input_group: false,
      in_audio_group: false,
      in_video_group: false,
      udev_rules_present: false,
    }
  }
  _deviceLoaded = true
}

export interface NormalizedAccessItem {
  id: 'ratbagd' | 'inputGroup' | 'audioGroup' | 'videoGroup' | 'udevRules'
  label: string
  status: boolean
}

export function getNormalizedAccessItems(access: DeviceAccessStatus): NormalizedAccessItem[] {
  return [
    { id: 'ratbagd', label: 'ratbagd', status: access.ratbagd_available },
    { id: 'inputGroup', label: 'input group', status: access.in_input_group },
    { id: 'audioGroup', label: 'audio group', status: access.in_audio_group },
    { id: 'videoGroup', label: 'video group', status: access.in_video_group },
    { id: 'udevRules', label: 'udev rules', status: access.udev_rules_present },
  ]
}

export { deps, deviceAccess, distroInfo }
