<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import { SERVICE_LABEL, useServicesStore } from '../stores/services'
import type { ServiceStatus } from '../types/ipc'

const props = defineProps<{ status: ServiceStatus }>()
const services = useServicesStore()

const STATE_LABEL = {
  stopped: 'Detenido',
  starting: 'Iniciando…',
  running: 'En marcha',
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

function toggle() {
  if (isUp.value) services.stop(props.status.id)
  else services.start(props.status.id)
}
</script>

<template>
  <li class="service" :data-state="status.state">
    <div class="service-main">
      <span class="dot" :title="STATE_LABEL[status.state]" />
      <div class="service-text">
        <span class="service-name">{{ SERVICE_LABEL[status.id] }}</span>
        <span class="service-meta">
          <template v-if="status.version">{{ status.version }}</template>
          <template v-else>sin instalar</template>
          · {{ status.id === 'php' ? `${status.port}+` : status.port }}
        </span>
      </div>
      <span class="service-state">{{ STATE_LABEL[status.state] }}</span>
      <button
        class="icon-btn"
        :disabled="busy || !isUp"
        :title="status.id === 'nginx' ? 'Recargar configuración' : 'Reiniciar'"
        @click="services.reload(status.id)"
      >
        <AppIcon name="refresh" />
      </button>
      <button
        class="icon-btn"
        :class="{ 'is-stop': isUp }"
        :disabled="busy"
        :title="isUp ? 'Detener' : 'Iniciar'"
        @click="toggle"
      >
        <AppIcon :name="isUp ? 'stop' : 'play'" />
      </button>
    </div>
    <p v-if="status.state === 'failed' && status.detail" class="service-detail">
      {{ status.detail }}
    </p>
  </li>
</template>
