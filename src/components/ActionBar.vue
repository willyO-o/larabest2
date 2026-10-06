<script setup lang="ts">
import { computed } from 'vue'
import AppIcon from './AppIcon.vue'
import { useServicesStore } from '../stores/services'
import { useSettingsStore } from '../stores/settings'

const services = useServicesStore()
const settings = useSettingsStore()

const busy = computed(() => services.pending.has('all'))
</script>

<template>
  <nav class="actions" aria-label="Acciones">
    <button
      class="action action-main"
      :class="{ 'is-stop': services.anyActive }"
      :disabled="busy"
      @click="services.anyActive ? services.stopAll() : services.startAll()"
    >
      <AppIcon :name="services.anyActive ? 'stop' : 'play'" />
      <span>{{ services.anyActive ? 'Detener' : 'Iniciar todo' }}</span>
    </button>
    <button class="action" @click="settings.openWeb()">
      <AppIcon name="globe" /><span>Web</span>
    </button>
    <button class="action" @click="settings.openDatabase()">
      <AppIcon name="database" /><span>Base de datos</span>
    </button>
    <button class="action" @click="settings.openTerminal()">
      <AppIcon name="terminal" /><span>Terminal</span>
    </button>
    <button class="action" @click="settings.openRoot()">
      <AppIcon name="folder" /><span>Raíz</span>
    </button>
  </nav>
</template>
