<script setup lang="ts">
import { computed } from 'vue'
import { SERVICE_LABEL, useServicesStore } from '../stores/services'
import type { ServiceStatus } from '../types/ipc'

const props = defineProps<{ status: ServiceStatus }>()
const services = useServicesStore()

const STATE_LABEL = {
  stopped: 'detenido',
  starting: 'iniciando…',
  running: 'en marcha',
  stopping: 'deteniendo…',
  failed: 'error',
} as const

const busy = computed(
  () =>
    services.pending.has(props.status.id) ||
    services.pending.has('all') ||
    props.status.state === 'starting' ||
    props.status.state === 'stopping',
)
const isUp = computed(() => props.status.state === 'running')

/** "puertos 9000–9003" si son seguidos (php-cgi), "puertos 1025/8025" si no. */
const port = computed(() => {
  const ports = [...(props.status.ports.length ? props.status.ports : [props.status.port])].sort(
    (a, b) => a - b,
  )
  const word = ports.length > 1 ? 'puertos' : 'puerto'
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
    <span class="service-name">{{ SERVICE_LABEL[status.id] }}</span>
    <span class="service-version">{{ status.version ?? 'sin instalar' }}</span>
    <span class="service-port" :title="port">{{ port }}</span>
    <span class="service-state">{{ STATE_LABEL[status.state] }}</span>
    <span class="service-actions">
      <button
        class="row-btn"
        :disabled="busy || !isUp"
        :title="status.id === 'nginx' ? 'Recargar la configuración sin cortar' : 'Reiniciar'"
        @click="services.reload(status.id)"
      >
        Recargar
      </button>
      <button class="row-btn" :class="{ 'is-stop': isUp }" :disabled="busy" @click="toggle">
        {{ isUp ? 'Detener' : 'Iniciar' }}
      </button>
    </span>
    <p v-if="status.state === 'failed' && status.detail" class="service-detail">
      {{ status.detail }}
    </p>
  </li>
</template>
