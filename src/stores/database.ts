import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { messageOf } from '../lib/errors'
import { EVENTS, type DatabaseInfo, type DatabaseMode } from '../types/ipc'

/** "2026-10-22_1130" con la hora local, para el nombre del respaldo. */
function localStamp(date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0')
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `_${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`
  )
}

/**
 * Menú MariaDB → Nueva base de datos, Respaldar y Cambiar contraseña de
 * root. Un solo panel con tres pestañas.
 */
export const useDatabaseStore = defineStore('database', () => {
  const open = ref(false)
  const mode = ref<DatabaseMode>('create')
  const info = ref<DatabaseInfo | null>(null)
  const loading = ref(false)
  const busy = ref(false)
  /** Error dentro del panel. */
  const error = ref<string | null>(null)
  /** Lo último que salió bien ("Se creó la base tienda"). */
  const done = ref<string | null>(null)
  /** Ruta del último respaldo, para abrir su carpeta. */
  const lastBackup = ref<string | null>(null)

  async function init() {
    await listen<DatabaseMode>(EVENTS.uiDatabase, (e) => show(e.payload))
  }

  async function refresh() {
    loading.value = true
    try {
      info.value = await invoke<DatabaseInfo>('db_info')
    } catch (e) {
      error.value = messageOf(e)
    } finally {
      loading.value = false
    }
  }

  async function show(next: DatabaseMode = 'create') {
    mode.value = next
    error.value = null
    done.value = null
    lastBackup.value = null
    open.value = true
    await refresh()
  }

  function setMode(next: DatabaseMode) {
    if (busy.value) return
    mode.value = next
    error.value = null
    done.value = null
  }

  function close() {
    if (!busy.value) open.value = false
  }

  async function run(action: () => Promise<string>) {
    busy.value = true
    error.value = null
    done.value = null
    try {
      done.value = await action()
      return true
    } catch (e) {
      error.value = messageOf(e)
      return false
    } finally {
      busy.value = false
    }
  }

  const create = (name: string) =>
    run(async () => {
      const created = await invoke<string>('db_create', { name })
      await refresh()
      return `Se creó la base de datos ${created} (utf8mb4).`
    })

  const backup = (name: string) =>
    run(async () => {
      const file = await invoke<string>('db_backup', { name, stamp: localStamp() })
      lastBackup.value = file
      return `Respaldo guardado: ${file}`
    })

  const setPassword = (password: string) =>
    run(async () => {
      await invoke('db_set_root_password', { password })
      await refresh()
      return password
        ? 'Listo. La app, Adminer, phpMyAdmin, HeidiSQL y los sitios nuevos ya usan la contraseña nueva.'
        : 'Listo. Root ya no tiene contraseña.'
    })

  async function openBackups() {
    try {
      await invoke('db_open_backups')
    } catch (e) {
      error.value = messageOf(e)
    }
  }

  return {
    open,
    mode,
    info,
    loading,
    busy,
    error,
    done,
    lastBackup,
    init,
    show,
    setMode,
    close,
    refresh,
    create,
    backup,
    setPassword,
    openBackups,
  }
})
