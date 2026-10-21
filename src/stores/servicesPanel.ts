import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { messageOf } from '../lib/errors'
import {
  EVENTS,
  type PortBusy,
  type PortSettings,
  type ServicesChoice,
  type ServicesSettings,
} from '../types/ipc'

/**
 * Preferencias → Servicios y puertos. Las filas de la ventana cambian con
 * `service:state`; aquí solo vive el formulario.
 */
export const useServicesPanelStore = defineStore('servicesPanel', () => {
  const open = ref(false)
  const current = ref<ServicesSettings | null>(null)
  /** Puertos del formulario que ya ocupa otro programa. */
  const busy = ref<PortBusy[]>([])
  const saving = ref(false)
  /** Error al guardar, dentro del panel (no detrás, en la ventana). */
  const error = ref<string | null>(null)

  async function init() {
    await listen(EVENTS.uiServices, () => show())
  }

  async function show() {
    error.value = null
    busy.value = []
    try {
      current.value = await invoke<ServicesSettings>('services_settings_get')
    } catch (e) {
      error.value = messageOf(e)
    }
    open.value = true
  }

  function close() {
    if (!saving.value) open.value = false
  }

  async function checkBusy(ports: PortSettings) {
    try {
      busy.value = await invoke<PortBusy[]>('services_settings_busy', { ports })
    } catch {
      busy.value = []
    }
  }

  async function save(choice: ServicesChoice) {
    saving.value = true
    error.value = null
    try {
      await invoke('services_settings_save', { choice })
      open.value = false
    } catch (e) {
      error.value = messageOf(e)
    } finally {
      saving.value = false
    }
  }

  return { open, current, busy, saving, error, init, show, close, checkBusy, save }
})
