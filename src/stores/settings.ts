import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'
import { attempt } from '../lib/errors'
import { EVENTS, type InstalledVersion, type Settings, type ThemePreference } from '../types/ipc'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings | null>(null)
  const appVersion = ref('')
  const phpVersions = ref<InstalledVersion[]>([])

  /** Tema elegido. Cambia solo cuando el núcleo lo confirma (`settings:theme`). */
  const theme = ref<ThemePreference>('system')

  async function init() {
    await listen<ThemePreference>(EVENTS.settingsTheme, (e) => {
      theme.value = e.payload
    })
    appVersion.value = (await attempt(() => getVersion())) ?? ''
    settings.value = (await attempt(() => invoke<Settings>('settings_get'))) ?? null
    if (settings.value) theme.value = settings.value.theme
    await loadPhpVersions()
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
    init,
    setTheme,
    loadPhpVersions,
    switchPhp,
    openRoot,
    openWeb,
    openDatabase,
    openTerminal,
  }
})
