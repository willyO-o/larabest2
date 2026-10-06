import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { EVENTS, type InstallProgress } from '../types/ipc'

/** Cuánto queda a la vista el resultado de una instalación. */
const DONE_MS = 6000
const FAILED_MS = 9000

/**
 * Paquete que se está agregando desde el menú (Agregar). Solo refleja los
 * eventos `install:progress`; el error completo lo muestra un diálogo.
 */
export const useInstallsStore = defineStore('installs', () => {
  const current = ref<InstallProgress | null>(null)
  let timer: ReturnType<typeof setTimeout> | undefined

  async function init() {
    await listen<InstallProgress>(EVENTS.installProgress, (e) => {
      clearTimeout(timer)
      current.value = e.payload
      if (e.payload.state !== 'running') {
        timer = setTimeout(
          () => (current.value = null),
          e.payload.state === 'done' ? DONE_MS : FAILED_MS,
        )
      }
    })
  }

  return { current, init }
})
