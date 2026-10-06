<script setup lang="ts">
import { computed, onMounted, watchEffect } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import ActionBar from './components/ActionBar.vue'
import AppIcon from './components/AppIcon.vue'
import BrandMark from './components/BrandMark.vue'
import ServiceRow from './components/ServiceRow.vue'
import ThemeSwitch from './components/ThemeSwitch.vue'
import { attempt, lastError } from './lib/errors'
import { SERVICE_LABEL, useServicesStore } from './stores/services'
import { useSettingsStore } from './stores/settings'
import { useSitesStore } from './stores/sites'

const services = useServicesStore()
const sites = useSitesStore()
const settings = useSettingsStore()

onMounted(async () => {
  await Promise.all([services.init(), sites.init(), settings.init()])
  // La ventana se crea oculta: se muestra cuando ya está pintada con su tema.
  requestAnimationFrame(() => attempt(() => invoke('app_ready')))
})

// El núcleo también cambia el tema nativo (barra de título y
// prefers-color-scheme); el atributo asegura el mismo resultado en la página.
watchEffect(() => {
  document.documentElement.dataset.theme = settings.theme
})

/** Estado de toda la pila: colorea la banda superior. */
const stack = computed(() => {
  const list = services.list
  if (list.some((s) => s.state === 'failed')) return 'failed'
  if (list.some((s) => s.state === 'starting' || s.state === 'stopping')) return 'busy'
  const running = list.filter((s) => s.state === 'running').length
  if (list.length && running === list.length) return 'running'
  return running ? 'partial' : 'stopped'
})

const summary = computed(() => {
  const list = services.list
  const running = list.filter((s) => s.state === 'running').length
  switch (stack.value) {
    case 'failed': {
      const names = list.filter((s) => s.state === 'failed').map((s) => SERVICE_LABEL[s.id])
      return `${names.join(' y ')} con error`
    }
    case 'busy':
      return list.some((s) => s.state === 'stopping') ? 'Deteniendo…' : 'Iniciando…'
    case 'running':
      return 'Todo en marcha'
    case 'partial':
      return `${running} de ${list.length} en marcha`
    default:
      return 'Detenido'
  }
})

const sitesLabel = computed(() => {
  const n = sites.list.length
  return n === 1 ? '1 sitio' : `${n} sitios`
})

/** Menú contextual nativo en árbol (el mismo que la bandeja). */
function openMenu(e?: MouseEvent) {
  const target = e?.currentTarget as HTMLElement | undefined
  if (target && e?.type === 'click') {
    const r = target.getBoundingClientRect()
    attempt(() => invoke('menu_popup', { x: r.left, y: r.bottom + 4 }))
  } else {
    attempt(() => invoke('menu_popup', {}))
  }
}
</script>

<template>
  <div class="app" @contextmenu.prevent="openMenu()">
    <header class="band" :data-stack="stack">
      <BrandMark />
      <span class="band-name">{{ settings.settings?.appName ?? 'LARABEST' }}</span>
      <span v-if="settings.appVersion" class="band-version">{{ settings.appVersion }}</span>
      <span class="band-summary" role="status">{{ summary }}</span>
      <ThemeSwitch />
      <button class="band-menu" aria-haspopup="menu" @click="openMenu($event)">
        Menú <AppIcon name="chevron" class="band-chev" />
      </button>
    </header>

    <ul class="services" aria-label="Servicios">
      <ServiceRow v-for="s in services.list" :key="s.id" :status="s" />
    </ul>

    <p v-if="lastError" class="error" role="alert">
      <AppIcon name="alert" />
      <span>{{ lastError }}</span>
      <button class="error-close" title="Cerrar" @click="lastError = null">
        <AppIcon name="close" />
      </button>
    </p>

    <ActionBar />

    <footer class="statusline">
      <span class="statusline-root" :title="settings.settings?.root">{{ settings.settings?.root }}</span>
      <span class="statusline-sep" aria-hidden="true">·</span>
      <span>{{ sitesLabel }} en www</span>
      <template v-if="sites.missingHosts.length">
        <span class="statusline-sep" aria-hidden="true">·</span>
        <button class="statusline-warn" :disabled="sites.syncing" @click="sites.sync()">
          {{ sites.missingHosts.length }} sin archivo hosts, reintentar
        </button>
      </template>
      <span class="statusline-hint">Clic derecho: más opciones</span>
    </footer>
  </div>
</template>
