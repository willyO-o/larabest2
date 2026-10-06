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
  <div class="action-bar">
    <button
      class="btn btn-primary"
      :class="{ 'is-stop': services.anyActive }"
      :disabled="busy"
      @click="services.anyActive ? services.stopAll() : services.startAll()"
    >
      <AppIcon :name="services.anyActive ? 'stop' : 'play'" />
      {{ services.anyActive ? 'Detener' : 'Iniciar todo' }}
    </button>
    <div class="action-grid">
      <button class="btn" @click="settings.openWeb()"><AppIcon name="globe" />Web</button>
      <button class="btn" @click="settings.openDatabase()">
        <AppIcon name="database" />Base de datos
      </button>
      <button class="btn" @click="settings.openTerminal()">
        <AppIcon name="terminal" />Terminal
      </button>
      <button class="btn" @click="settings.openRoot()"><AppIcon name="folder" />Raíz</button>
    </div>
  </div>
</template>
