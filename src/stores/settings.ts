import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'
import { attempt } from '../lib/errors'
import {
  EVENTS,
  type InstalledVersion,
  type Panel,
  type PortSettings,
  type Settings,
  type ThemePreference,
} from '../types/ipc'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings | null>(null)
  const appVersion = ref('')
  const phpVersions = ref<InstalledVersion[]>([])

  /** Tema elegido. Cambia solo cuando el núcleo lo confirma (`settings:theme`). */
  const theme = ref<ThemePreference>('system')
  /** HTTPS activo para los sitios. Cambia con `settings:ssl`. */
  const ssl = ref(false)
  /** Panel abierto por el menú (puertos o contraseña de root). */
  const panel = ref<Panel | null>(null)

  async function init() {
    await listen<ThemePreference>(EVENTS.settingsTheme, (e) => {
      theme.value = e.payload
    })
    await listen<boolean>(EVENTS.settingsSsl, (e) => {
      ssl.value = e.payload
    })
    await listen<Panel>(EVENTS.uiPanel, (e) => {
      panel.value = e.payload
    })
    await listen(EVENTS.settingsChanged, () => reload())
    appVersion.value = (await attempt(() => getVersion())) ?? ''
    await reload()
    await loadPhpVersions()
  }

  async function reload() {
    settings.value = (await attempt(() => invoke<Settings>('settings_get'))) ?? settings.value
    if (settings.value) {
      theme.value = settings.value.theme
      ssl.value = settings.value.ssl
    }
  }

  const getPorts = () => invoke<PortSettings>('settings_get_ports')
  /** Rechaza con el error del núcleo (para mostrarlo en el panel). */
  async function savePorts(ports: PortSettings) {
    await invoke('settings_set_ports', { ports })
    await reload()
  }
  async function saveRootPassword(password: string) {
    const message = await invoke<string>('settings_set_root_password', { password })
    await reload()
    return message
  }

  const setTheme = (value: ThemePreference) =>
    attempt(() => invoke('settings_set_theme', { theme: value }))

  async function loadPhpVersions() {
    phpVersions.value = (await attempt(() => invoke<InstalledVersion[]>('php_list_versions'))) ?? []
  }

  const switchPhp = (version: string) => attempt(() => invoke('php_switch_version', { version }))

  const openRoot = () => attempt(() => invoke('shell_open_root'))
  const openWeb = () => attempt(() => invoke('shell_open_web'))
  const openDatabase = () => attempt(() => invoke('shell_open_database'))
  const openTerminal = () => attempt(() => invoke('shell_open_terminal', { cwd: null }))

  return {
    settings,
    appVersion,
    phpVersions,
    theme,
    ssl,
    panel,
    init,
    reload,
    getPorts,
    savePorts,
    saveRootPassword,
    setTheme,
    loadPhpVersions,
    switchPhp,
    openRoot,
    openWeb,
    openDatabase,
    openTerminal,
  }
})
