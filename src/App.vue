<script setup lang="ts">
import { computed, onMounted, watch, watchEffect } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import ActionBar from './components/ActionBar.vue'
import AppIcon from './components/AppIcon.vue'
import BrandMark from './components/BrandMark.vue'
import CreateSitePanel from './components/CreateSitePanel.vue'
import DatabasePanel from './components/DatabasePanel.vue'
import ServiceRow from './components/ServiceRow.vue'
import ServicesPanel from './components/ServicesPanel.vue'
import SetupPanel from './components/SetupPanel.vue'
import ThemeSwitch from './components/ThemeSwitch.vue'
import { attempt, lastError } from './lib/errors'
import { progressDetail, progressText } from './lib/progress'
import { SERVICE_LABEL, useServicesStore } from './stores/services'
import { useDatabaseStore } from './stores/database'
import { useInstallsStore } from './stores/installs'
import { useServicesPanelStore } from './stores/servicesPanel'
import { useSetupStore } from './stores/setup'
import { useSettingsStore } from './stores/settings'
import { useSitesStore } from './stores/sites'

const services = useServicesStore()
const sites = useSitesStore()
const settings = useSettingsStore()
const installs = useInstallsStore()
const servicesPanel = useServicesPanelStore()
const setup = useSetupStore()
const database = useDatabaseStore()

onMounted(async () => {
  await Promise.all([
    services.init(),
    sites.init(),
    settings.init(),
    installs.init(),
    servicesPanel.init(),
    setup.init(),
    database.init(),
  ])
  // La ventana se crea oculta: se muestra cuando ya está pintada con su tema.
  requestAnimationFrame(() => attempt(() => invoke('app_ready')))
})

/** Alto de una fila de servicio (`.service` en styles.css). */
const ROW_HEIGHT = 48
/** Alto de la ventana con las tres filas de la pila base. */
const BASE_HEIGHT = 380
/** Alto mínimo con Servicios y puertos abierto: entran todas sus filas. */
const SERVICES_PANEL_HEIGHT = 590
/** Alto mínimo con el panel de bases de datos. */
const DATABASE_PANEL_HEIGHT = 480
/** Alto mínimo con el asistente abierto (su lista se desplaza si no entra). */
const SETUP_PANEL_HEIGHT = 620

// Cada servicio opcional activo suma una fila: la ventana crece con ellas.
// Con Servicios y puertos o el asistente abiertos crece lo necesario y al
// cerrarlos vuelve.
watch(
  [() => services.list.length, () => servicesPanel.open, () => setup.open, () => database.open],
  ([rows, panelOpen, setupOpen, databaseOpen]) => {
    if (!rows) return
    let height = BASE_HEIGHT + Math.max(0, rows - 3) * ROW_HEIGHT
    if (panelOpen) height = Math.max(height, SERVICES_PANEL_HEIGHT)
    if (databaseOpen) height = Math.max(height, DATABASE_PANEL_HEIGHT)
    if (setupOpen) height = Math.max(height, SETUP_PANEL_HEIGHT)
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
      <!-- Como en el logo: "Lara" en el color del texto y "Best" en azul. -->
      <span class="band-name" aria-label="LaraBest">
        <span aria-hidden="true">Lara</span
        ><span class="band-name-accent" aria-hidden="true">Best</span>
      </span>
      <span v-if="settings.appVersion" class="band-version">{{ settings.appVersion }}</span>
      <span class="band-summary" role="status">{{ summary }}</span>
      <ThemeSwitch />
      <button
        class="band-icon"
        aria-label="Editar archivo hosts"
        title="Editar archivo hosts"
        @click="attempt(() => invoke('shell_edit_hosts'))"
      >
        <AppIcon name="hosts" />
      </button>
      <button
        class="band-icon"
        aria-label="Servicios y puertos"
        title="Servicios y puertos"
        :aria-expanded="servicesPanel.open"
        @click="servicesPanel.open ? servicesPanel.close() : servicesPanel.show()"
      >
        <AppIcon name="gear" />
      </button>
      <span class="band-divider" aria-hidden="true" />
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
          :title="progressDetail(installs.current)"
        >
          <span v-if="installs.current.state === 'running'" class="task-dot" aria-hidden="true" />
          <AppIcon v-else :name="installs.current.state === 'done' ? 'check' : 'alert'" />
          <span class="task-text">{{
            installs.current.state === 'done'
              ? installs.current.detail
              : installs.current.state === 'failed'
                ? `No se pudo agregar ${installs.current.label}`
                : progressText(installs.current)
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
    <ServicesPanel />
    <SetupPanel />
    <DatabasePanel />
  </div>
</template>
