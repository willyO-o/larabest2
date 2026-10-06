<script setup lang="ts">
import { computed, onMounted, watch, watchEffect } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import ActionBar from './components/ActionBar.vue'
import AppIcon from './components/AppIcon.vue'
import BrandMark from './components/BrandMark.vue'
import CreateSitePanel from './components/CreateSitePanel.vue'
import ServiceRow from './components/ServiceRow.vue'
import ThemeSwitch from './components/ThemeSwitch.vue'
import { attempt, lastError } from './lib/errors'
import { SERVICE_LABEL, useServicesStore } from './stores/services'
import { useInstallsStore } from './stores/installs'
import { useSettingsStore } from './stores/settings'
import { useSitesStore } from './stores/sites'

const services = useServicesStore()
const sites = useSitesStore()
const settings = useSettingsStore()
const installs = useInstallsStore()

onMounted(async () => {
  await Promise.all([services.init(), sites.init(), settings.init(), installs.init()])
  // La ventana se crea oculta: se muestra cuando ya está pintada con su tema.
  requestAnimationFrame(() => attempt(() => invoke('app_ready')))
})

/** Alto de una fila de servicio (`.service` en styles.css). */
const ROW_HEIGHT = 46
/** Alto de la ventana con las tres filas de la pila base. */
const BASE_HEIGHT = 380

// Cada servicio opcional activo suma una fila: la ventana crece con ellas.
watch(
  () => services.list.length,
  (rows) => {
    if (!rows) return
    const height = BASE_HEIGHT + Math.max(0, rows - 3) * ROW_HEIGHT
    attempt(() => invoke('window_fit', { height }))
  },
  { immediate: true },
)

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
      <!-- Mientras se agrega un paquete, su progreso ocupa este lugar. -->
      <template v-if="!installs.current">
        <span class="statusline-root" :title="settings.settings?.root">{{
          settings.settings?.root
        }}</span>
        <span class="statusline-sep" aria-hidden="true">·</span>
        <span>{{ sitesLabel }} en www</span>
      </template>
      <template v-if="settings.ssl && !installs.current">
        <span class="statusline-sep" aria-hidden="true">·</span>
        <span class="statusline-https"><AppIcon name="lock" />HTTPS</span>
      </template>
      <template v-if="sites.missingHosts.length">
        <span v-if="!installs.current" class="statusline-sep" aria-hidden="true">·</span>
        <button class="statusline-warn" :disabled="sites.syncing" @click="sites.sync()">
          {{ sites.missingHosts.length }} sin archivo hosts, reintentar
        </button>
      </template>
      <template v-if="installs.current">
        <span v-if="sites.missingHosts.length" class="statusline-sep" aria-hidden="true">·</span>
        <span
          class="statusline-task"
          :data-state="installs.current.state"
          role="status"
          :title="installs.current.detail ?? installs.current.step"
        >
          <span v-if="installs.current.state === 'running'" class="task-dot" aria-hidden="true" />
          <AppIcon v-else :name="installs.current.state === 'done' ? 'check' : 'alert'" />
          <span class="task-text">{{
            installs.current.state === 'done'
              ? installs.current.detail
              : installs.current.state === 'failed'
                ? `No se pudo agregar ${installs.current.label}`
                : `${installs.current.step}…`
          }}</span>
        </span>
      </template>
      <button class="statusline-new" @click="sites.openCreate()">
        <AppIcon name="plus" />Nuevo sitio
      </button>
      <span v-if="!sites.missingHosts.length && !installs.current" class="statusline-hint"
        >Clic derecho: menú</span
      >
    </footer>

    <CreateSitePanel />
  </div>
</template>
