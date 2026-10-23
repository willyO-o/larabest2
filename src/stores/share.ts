import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { messageOf } from '../lib/errors'
import { EVENTS, type ShareInfo } from '../types/ipc'

/**
 * Menú www → Compartir en la red local. Cada proyecto compartido se ve en
 * `http://<IP del equipo>:<puerto>/` desde otro equipo o el celular en la
 * misma red.
 */
export const useShareStore = defineStore('share', () => {
  const open = ref(false)
  const info = ref<ShareInfo | null>(null)
  /** Proyecto que se está compartiendo o dejando de compartir. */
  const busy = ref<string | null>(null)
  const error = ref<string | null>(null)
  /** Última dirección copiada (para el "Copiado"). */
  const copied = ref<string | null>(null)

  const shared = computed(() => info.value?.sites.filter((s) => s.shared) ?? [])

  async function refresh() {
    try {
      info.value = await invoke<ShareInfo>('share_info')
    } catch (e) {
      error.value = messageOf(e)
    }
  }

  async function init() {
    await listen(EVENTS.uiShare, () => show())
    // Los proyectos cambian (crear, borrar) y el servidor web arranca o
    // se detiene: la lista se pone al día.
    await listen(EVENTS.sitesChanged, () => refresh())
    await listen(EVENTS.serviceState, () => {
      if (open.value || shared.value.length) refresh()
    })
    await refresh()
  }

  async function show() {
    error.value = null
    copied.value = null
    open.value = true
    await refresh()
  }

  function close() {
    if (!busy.value) open.value = false
  }

  async function toggle(name: string, value: boolean) {
    busy.value = name
    error.value = null
    try {
      info.value = await invoke<ShareInfo>('share_set', { name, shared: value })
    } catch (e) {
      error.value = messageOf(e)
      await refresh()
    } finally {
      busy.value = null
    }
  }

  async function fixWordpress(name: string) {
    busy.value = name
    error.value = null
    try {
      info.value = await invoke<ShareInfo>('share_fix_wordpress', { name })
    } catch (e) {
      error.value = messageOf(e)
    } finally {
      busy.value = null
    }
  }

  async function openSite(name: string) {
    try {
      await invoke('share_open', { name })
    } catch (e) {
      error.value = messageOf(e)
    }
  }

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url)
      copied.value = url
      setTimeout(() => {
        if (copied.value === url) copied.value = null
      }, 1800)
    } catch {
      error.value = 'No se pudo copiar: selecciona la dirección y cópiala a mano.'
    }
  }

  return {
    open,
    info,
    busy,
    error,
    copied,
    shared,
    init,
    show,
    close,
    toggle,
    fixWordpress,
    openSite,
    copy,
    refresh,
  }
})
