import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { messageOf } from '../lib/errors'
import { EVENTS, type SetupInfo } from '../types/ipc'

export type SetupItemState = 'running' | 'done' | 'failed'

export interface SetupResult {
  state: SetupItemState
  message?: string
}

/**
 * Asistente "Preparar LARABEST": se abre solo la primera vez y desde Menú →
 * Agregar. Instala los paquetes de uno en uno; el paso en curso llega con
 * `install:progress` (store `installs`).
 */
export const useSetupStore = defineStore('setup', () => {
  const open = ref(false)
  const info = ref<SetupInfo | null>(null)
  const results = ref<Record<string, SetupResult>>({})
  const installing = ref(false)
  /** Ya se instaló lo elegido: queda el resumen y el botón Cerrar. */
  const finished = ref(false)
  /** Error al leer el catálogo. */
  const error = ref<string | null>(null)
  /** Instalando Homebrew o MacPorts. */
  const installingManager = ref(false)
  const managerError = ref<string | null>(null)

  async function load() {
    try {
      info.value = await invoke<SetupInfo>('setup_info')
      error.value = null
    } catch (e) {
      error.value = messageOf(e)
    }
  }

  async function init() {
    await listen(EVENTS.uiSetup, () => show())
    await load()
    if (info.value?.show) open.value = true
  }

  async function show() {
    results.value = {}
    finished.value = false
    await load()
    open.value = true
  }

  async function install(ids: string[]) {
    installing.value = true
    for (const id of ids) {
      results.value[id] = { state: 'running' }
      try {
        const message = await invoke<string>('packages_install', { id })
        results.value[id] = { state: 'done', message }
      } catch (e) {
        results.value[id] = { state: 'failed', message: messageOf(e) }
      }
    }
    installing.value = false
    finished.value = true
    // Estados "instalado" al día.
    await load()
  }

  /** macOS: instala Homebrew o MacPorts antes que lo demás. */
  async function installManager() {
    installingManager.value = true
    managerError.value = null
    try {
      await invoke<string>('package_manager_install')
    } catch (e) {
      managerError.value = messageOf(e)
    } finally {
      installingManager.value = false
      await load()
    }
  }

  /** Cerrar u omitir: no se vuelve a abrir solo. */
  async function close() {
    if (installing.value || installingManager.value) return
    try {
      await invoke('setup_finish')
    } catch (e) {
      error.value = messageOf(e)
      return
    }
    open.value = false
  }

  return {
    open,
    info,
    results,
    installing,
    finished,
    error,
    installingManager,
    managerError,
    init,
    show,
    install,
    installManager,
    close,
  }
})
