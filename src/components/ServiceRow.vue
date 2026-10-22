<script setup lang="ts">
import { computed, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import ServiceLogo from './ServiceLogo.vue'
import { SERVICE_LABEL, useServicesStore } from '../stores/services'
import { useSettingsStore } from '../stores/settings'
import type { ServiceId, ServiceStatus } from '../types/ipc'

const props = defineProps<{ status: ServiceStatus }>()
const services = useServicesStore()
const settings = useSettingsStore()

// PHP muestra cómo está compilado (NTS · x64 · VS17): importa para elegir
// extensiones. Se pide al cambiar de versión.
watch(
  () => (props.status.id === 'php' ? props.status.version : null),
  (version) => {
    if (version) settings.loadPhpBuild(version)
  },
  { immediate: true },
)

const build = computed(() =>
  props.status.id === 'php' && props.status.version
    ? (settings.phpBuilds[props.status.version] ?? null)
    : null,
)

/** "NTS · x64 · VS17". */
const buildShort = computed(() => {
  const b = build.value
  if (!b) return ''
  return [b.threadSafe ? 'TS' : 'NTS', b.arch, b.toolset].filter(Boolean).join(' · ')
})

/** Detalle completo al pasar el cursor. */
const buildTitle = computed(() => {
  const b = build.value
  if (!b) return ''
  const lines = [
    `PHP ${b.version}`,
    b.threadSafe ? 'Thread Safe (TS)' : 'Non Thread Safe (NTS): un proceso php-cgi por petición, vía FastCGI',
    b.arch === 'x64' ? 'Arquitectura: 64 bits (x64)' : b.arch ? `Arquitectura: ${b.arch}` : null,
    b.compiler ? `Compilador: ${b.compiler}${b.toolset ? ` (${b.toolset})` : ''}` : null,
    b.extensionApi ? `API de extensiones: ${b.extensionApi}` : null,
    b.toolset
      ? `Las extensiones (.dll) tienen que ser ${b.threadSafe ? 'TS' : 'NTS'} ${b.arch ?? ''} ${b.toolset}.`
      : null,
  ]
  return lines.filter(Boolean).join('\n')
})

const STATE_LABEL = {
  stopped: 'Detenido',
  starting: 'Iniciando…',
  running: 'Ejecutándose',
  stopping: 'Deteniendo…',
  failed: 'Error',
} as const

const busy = computed(
  () =>
    services.pending.has(props.status.id) ||
    services.pending.has('all') ||
    props.status.state === 'starting' ||
    props.status.state === 'stopping',
)
const isUp = computed(() => props.status.state === 'running')

/** Los servidores web recargan su configuración; el resto se reinicia. */
const RELOAD_TITLE: Partial<Record<ServiceId, string>> = {
  nginx: 'Recargar la configuración sin cortar',
  apache: 'Recargar la configuración',
}

/** "puertos 9000–9003" si son seguidos (php-cgi), "puertos 1025/8025" si no. */
const port = computed(() => {
  const ports = [...(props.status.ports.length ? props.status.ports : [props.status.port])].sort(
    (a, b) => a - b,
  )
  const word = ports.length > 1 ? 'Puertos' : 'Puerto'
  const consecutive = ports.every((p, i) => i === 0 || p === ports[i - 1] + 1)
  if (ports.length > 2 && consecutive) return `${word} ${ports[0]}–${ports[ports.length - 1]}`
  return `${word} ${ports.join('/')}`
})

function toggle() {
  if (isUp.value) services.stop(props.status.id)
  else services.start(props.status.id)
}
</script>

<template>
  <li class="service" :data-state="status.state">
    <span class="dot" aria-hidden="true" />
    <ServiceLogo :id="status.id" />
    <span class="service-name">{{ SERVICE_LABEL[status.id] }}</span>
    <span class="service-meta">
      <span class="service-version">{{ status.version ?? 'sin instalar' }}</span>
      <span v-if="buildShort" class="service-build" :title="buildTitle">{{ buildShort }}</span>
      <span class="service-sep" aria-hidden="true">·</span>
      <span class="service-port" :title="port">{{ port }}</span>
    </span>
    <span class="service-state"><span class="state-dot" aria-hidden="true" />{{ STATE_LABEL[status.state] }}</span>
    <span class="service-actions">
      <button
        class="row-btn"
        :class="isUp ? 'is-stop' : 'is-start'"
        :disabled="busy"
        @click="toggle"
      >
        <AppIcon :name="isUp ? 'stop' : 'play'" />{{ isUp ? 'Detener' : 'Iniciar' }}
      </button>
      <button
        class="row-icon"
        :disabled="busy || !isUp"
        :title="RELOAD_TITLE[status.id] ?? 'Reiniciar'"
        :aria-label="`Recargar ${SERVICE_LABEL[status.id]}`"
        @click="services.reload(status.id)"
      >
        <AppIcon name="refresh" />
      </button>
    </span>
    <p v-if="status.state === 'failed' && status.detail" class="service-detail">
      {{ status.detail }}
    </p>
  </li>
</template>
