import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { attempt } from '../lib/errors'
import { EVENTS, type ServiceId, type ServiceStatus } from '../types/ipc'

export const SERVICE_ORDER: ServiceId[] = [
  'nginx',
  'php',
  'mariadb',
  'postgresql',
  'redis',
  'mongodb',
  'mailpit',
]

export const SERVICE_LABEL: Record<ServiceId, string> = {
  nginx: 'Nginx',
  php: 'PHP',
  mariadb: 'MariaDB',
  postgresql: 'PostgreSQL',
  redis: 'Redis',
  mongodb: 'MongoDB',
  mailpit: 'Mailpit',
}

/**
 * Estado de los servicios. Solo cambia al recibir `service:state`: la
 * interfaz nunca supone que un comando funcionó.
 */
export const useServicesStore = defineStore('services', () => {
  const byId = ref<Partial<Record<ServiceId, ServiceStatus>>>({})
  /** Acciones en curso, solo para deshabilitar botones mientras esperan. */
  const pending = ref(new Set<string>())

  async function init() {
    await listen<ServiceStatus>(EVENTS.serviceState, (e) => {
      byId.value[e.payload.id] = e.payload
    })
    const snapshot = await attempt(() => invoke<ServiceStatus[]>('services_snapshot'))
    for (const s of snapshot ?? []) byId.value[s.id] = s
  }

  /** Las filas: la pila activa y, por si acaso, cualquiera que siga en marcha. */
  const list = computed(() =>
    SERVICE_ORDER.map((id) => byId.value[id]).filter(
      (s): s is ServiceStatus => !!s && (s.enabled || s.state !== 'stopped'),
    ),
  )

  const anyActive = computed(() =>
    list.value.some((s) => s.state === 'running' || s.state === 'starting' || s.state === 'stopping'),
  )

  async function track(key: string, fn: () => Promise<unknown>) {
    pending.value.add(key)
    try {
      await attempt(fn)
    } finally {
      pending.value.delete(key)
    }
  }

  const startAll = () => track('all', () => invoke('services_start_all'))
  const stopAll = () => track('all', () => invoke('services_stop_all'))
  const start = (id: ServiceId) => track(id, () => invoke('services_start', { id }))
  const stop = (id: ServiceId) => track(id, () => invoke('services_stop', { id }))
  const reload = (id: ServiceId) => track(id, () => invoke('services_reload', { id }))

  return { byId, list, pending, anyActive, init, startAll, stopAll, start, stop, reload }
})
