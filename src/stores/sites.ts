import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { attempt } from '../lib/errors'
import { EVENTS, type Site } from '../types/ipc'

export const useSitesStore = defineStore('sites', () => {
  const list = ref<Site[]>([])
  const syncing = ref(false)

  async function init() {
    await listen<Site[]>(EVENTS.sitesChanged, (e) => {
      list.value = e.payload
    })
    list.value = (await attempt(() => invoke<Site[]>('sites_list'))) ?? []
  }

  /** Sitios cuyo dominio falta en el archivo hosts. */
  const missingHosts = computed(() => list.value.filter((s) => !s.inHosts))

  async function sync() {
    syncing.value = true
    try {
      await attempt(() => invoke<Site[]>('sites_sync'))
    } finally {
      syncing.value = false
    }
  }

  const open = (name: string) => attempt(() => invoke('shell_open_site', { name }))
  const openFolder = (name: string) => attempt(() => invoke('shell_open_site_folder', { name }))
  const openTerminal = (cwd: string) => attempt(() => invoke('shell_open_terminal', { cwd }))

  return { list, syncing, missingHosts, init, sync, open, openFolder, openTerminal }
})
