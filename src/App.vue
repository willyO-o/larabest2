<script setup lang="ts">
import { onMounted, ref } from 'vue'
import ActionBar from './components/ActionBar.vue'
import AppIcon from './components/AppIcon.vue'
import ServiceRow from './components/ServiceRow.vue'
import TreeMenu from './components/TreeMenu.vue'
import { lastError } from './lib/errors'
import { useServicesStore } from './stores/services'
import { useSettingsStore } from './stores/settings'
import { useSitesStore } from './stores/sites'

const services = useServicesStore()
const sites = useSitesStore()
const settings = useSettingsStore()
const menuOpen = ref(false)

onMounted(async () => {
  await Promise.all([services.init(), sites.init(), settings.init()])
})
</script>

<template>
  <div class="app">
    <header class="app-header">
      <div class="brand">
        <span class="brand-name">{{ settings.settings?.appName ?? 'APP_NAME' }}</span>
        <span v-if="settings.appVersion" class="brand-version">v{{ settings.appVersion }}</span>
      </div>
      <button
        class="icon-btn"
        :aria-expanded="menuOpen"
        title="Menú"
        @click="menuOpen = !menuOpen"
      >
        <AppIcon name="menu" />
      </button>
    </header>

    <main class="app-body">
      <ul class="services">
        <ServiceRow v-for="s in services.list" :key="s.id" :status="s" />
      </ul>
      <p v-if="sites.list.length" class="sites-summary">
        {{ sites.list.length === 1 ? '1 sitio' : `${sites.list.length} sitios` }} en www ·
        <button class="link" @click="menuOpen = true">ver</button>
      </p>
    </main>

    <p v-if="lastError" class="error-banner" role="alert">
      <AppIcon name="alert" />
      <span>{{ lastError }}</span>
      <button class="icon-btn" title="Cerrar" @click="lastError = null">
        <AppIcon name="close" />
      </button>
    </p>

    <ActionBar />

    <Transition name="fade">
      <div v-if="menuOpen" class="scrim" @click="menuOpen = false" />
    </Transition>
    <Transition name="panel">
      <TreeMenu v-if="menuOpen" class="tree-panel" @close="menuOpen = false" />
    </Transition>
  </div>
</template>
