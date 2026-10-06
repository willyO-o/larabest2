import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { attempt } from '../lib/errors'
import type { InstalledVersion, Settings } from '../types/ipc'

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings | null>(null)
  const appVersion = ref('')
  const phpVersions = ref<InstalledVersion[]>([])
  /** Carpeta elegida que se aplicará al reiniciar la app. */
  const pendingRoot = ref<string | null>(null)

  async function init() {
    appVersion.value = (await attempt(() => getVersion())) ?? ''
    settings.value = (await attempt(() => invoke<Settings>('settings_get'))) ?? null
    await loadPhpVersions()
  }

  async function loadPhpVersions() {
    phpVersions.value = (await attempt(() => invoke<InstalledVersion[]>('php_list_versions'))) ?? []
  }

  const switchPhp = (version: string) => attempt(() => invoke('php_switch_version', { version }))

  async function chooseRoot() {
    const chosen = await attempt(() => invoke<string | null>('settings_choose_root'))
    if (chosen) pendingRoot.value = chosen
  }

  const openRoot = () => attempt(() => invoke('shell_open_root'))
  const openWeb = () => attempt(() => invoke('shell_open_web'))
  const openDatabase = () => attempt(() => invoke('shell_open_database'))
  const openTerminal = () => attempt(() => invoke('shell_open_terminal', { cwd: null }))

  return {
    settings,
    appVersion,
    phpVersions,
    pendingRoot,
    init,
    loadPhpVersions,
    switchPhp,
    chooseRoot,
    openRoot,
    openWeb,
    openDatabase,
    openTerminal,
  }
})
