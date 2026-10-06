import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { attempt, report } from '../lib/errors'
import { EVENTS, type Site, type SitePreset, type SiteProgress } from '../types/ipc'

/** Estado del formulario de sitio nuevo, alimentado por `sites:progress`. */
export interface Creation {
  name: string
  preset: SitePreset
  steps: SiteProgress[]
  /** Sitio creado, cuando termina bien. */
  site: Site | null
  /** Mensaje de error si falló. */
  error: string | null
  /** Aviso al terminar (por ejemplo, migraciones que fallaron). */
  warning: string | null
}

export const useSitesStore = defineStore('sites', () => {
  const list = ref<Site[]>([])
  const syncing = ref(false)

  /** Formulario abierto y plantilla preseleccionada. */
  const createOpen = ref(false)
  const createPreset = ref<SitePreset>('blank')
  const creation = ref<Creation | null>(null)
  const creating = computed(() => !!creation.value && !creation.value.site && !creation.value.error)

  async function init() {
    await listen<Site[]>(EVENTS.sitesChanged, (e) => {
      list.value = e.payload
    })
    await listen<SiteProgress>(EVENTS.sitesProgress, (e) => {
      const c = creation.value
      if (!c || c.name !== e.payload.name) return
      const last = c.steps[c.steps.length - 1]
      // Un fallo marca el paso en curso; no es un paso nuevo.
      if (e.payload.state === 'failed') {
        if (last && last.state === 'running') {
          last.state = 'failed'
          last.detail = e.payload.detail
        }
        return
      }
      // Un paso nuevo cierra el anterior.
      if (last && last.state === 'running') last.state = 'done'
      // El cierre ("Sitio listo") no es un paso: el título ya lo dice.
      if (e.payload.state === 'done') {
        c.warning = e.payload.detail ?? null
        return
      }
      c.steps.push(e.payload)
    })
    await listen<SitePreset>(EVENTS.uiCreateSite, (e) => openCreate(e.payload))
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

  /** Vuelve al formulario tras un error, conservando nombre y plantilla. */
  function backToForm() {
    if (!creating.value) creation.value = null
  }

  function openCreate(preset: SitePreset = 'blank') {
    if (creating.value) {
      createOpen.value = true
      return
    }
    creation.value = null
    createPreset.value = preset
    createOpen.value = true
  }

  function closeCreate() {
    if (creating.value) return
    createOpen.value = false
    creation.value = null
  }

  async function create(name: string, preset: SitePreset) {
    creation.value = { name, preset, steps: [], site: null, error: null, warning: null }
    try {
      const site = await invoke<Site>('sites_create', { name, preset })
      if (creation.value?.name === name) {
        const last = creation.value.steps[creation.value.steps.length - 1]
        if (last && last.state === 'running') last.state = 'done'
        creation.value.site = site
      }
    } catch (e) {
      if (creation.value?.name === name) {
        const last = creation.value.steps[creation.value.steps.length - 1]
        if (last && last.state === 'running') last.state = 'failed'
        creation.value.error = (e as { message?: string })?.message ?? String(e)
      } else {
        report(e)
      }
    }
  }

  const open = (name: string) => attempt(() => invoke('shell_open_site', { name }))
  const openFolder = (name: string) => attempt(() => invoke('shell_open_site_folder', { name }))
  const openTerminal = (cwd: string) => attempt(() => invoke('shell_open_terminal', { cwd }))

  return {
    list,
    syncing,
    missingHosts,
    createOpen,
    createPreset,
    creation,
    creating,
    init,
    sync,
    openCreate,
    closeCreate,
    backToForm,
    create,
    open,
    openFolder,
    openTerminal,
  }
})
