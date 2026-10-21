<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useInstallsStore } from '../stores/installs'
import { useSetupStore } from '../stores/setup'
import type { SetupPackage } from '../types/ipc'

const setup = useSetupStore()
const installs = useInstallsStore()

const GROUPS = ['Pila base', 'Servicios opcionales', 'Herramientas']

const dialog = ref<HTMLDialogElement>()
const mainButton = ref<HTMLButtonElement>()
const selected = reactive(new Set<string>())

const packages = computed(() => setup.info?.packages ?? [])
const groups = computed(() =>
  GROUPS.map((label, group) => ({
    label,
    items: packages.value.filter((p) => p.group === group),
  })).filter((g) => g.items.length),
)

/** Lo recomendado que falta, cada vez que se abre. */
function preselect() {
  selected.clear()
  for (const p of packages.value) {
    if (p.recommended && !p.status) selected.add(p.id)
  }
}

watch(
  () => setup.open,
  async (open) => {
    if (!open) {
      dialog.value?.close()
      return
    }
    preselect()
    if (!dialog.value?.open) dialog.value?.showModal()
    await nextTick()
    mainButton.value?.focus()
  },
)

// El catálogo puede llegar después de abrir (primer arranque).
watch(packages, () => {
  if (setup.open && !setup.installing && !setup.finished) preselect()
})

function toggle(p: SetupPackage) {
  if (selected.has(p.id)) selected.delete(p.id)
  else selected.add(p.id)
}

/** En el orden del catálogo: la pila base antes que las herramientas. */
const queue = computed(() => packages.value.filter((p) => selected.has(p.id)).map((p) => p.id))

const progress = computed(() => {
  const ids = Object.keys(setup.results)
  const done = ids.filter((id) => setup.results[id].state !== 'running').length
  return { done, total: queue.value.length }
})

const failures = computed(
  () => Object.values(setup.results).filter((r) => r.state === 'failed').length,
)

function statusText(p: SetupPackage): string {
  const result = setup.results[p.id]
  if (result?.state === 'running') {
    const step = installs.current?.id === p.id ? installs.current.step : null
    return step ? `${step}…` : 'Instalando…'
  }
  if (result) return result.message ?? ''
  if (!p.status) return ''
  // Las herramientas sin versión dicen solo "instalado".
  return p.status === 'instalado' ? p.status : `instalado · ${p.status}`
}

function rowState(p: SetupPackage) {
  return setup.results[p.id]?.state ?? (p.status ? 'installed' : 'idle')
}

function start() {
  if (queue.value.length && !setup.installing) setup.install(queue.value)
}

function onCancel(e: Event) {
  e.preventDefault()
  setup.close()
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="setup-title" @cancel="onCancel">
    <div class="sheet-body">
      <header class="sheet-head">
        <h2 id="setup-title">Preparar LARABEST</h2>
        <button
          type="button"
          class="sheet-close"
          title="Cerrar"
          :disabled="setup.installing"
          @click="setup.close()"
        >
          <AppIcon name="close" />
        </button>
      </header>

      <p class="setup-intro">
        Para <strong>{{ setup.info?.os ?? 'tu sistema' }}</strong>: se descarga de los sitios
        oficiales la versión de cada programa que funciona en tu equipo.
      </p>

      <div class="setup-list">
        <section v-for="g in groups" :key="g.label" class="setup-group">
          <h3 class="setup-group-title">{{ g.label }}</h3>
          <label
            v-for="p in g.items"
            :key="p.id"
            class="setup-row"
            :data-state="rowState(p)"
          >
            <input
              type="checkbox"
              :checked="selected.has(p.id) || !!p.status"
              :disabled="!!p.status || setup.installing || setup.finished"
              @change="toggle(p)"
            />
            <span class="setup-name">{{ p.label }}</span>
            <span class="setup-status" :title="statusText(p)">
              <span
                v-if="setup.results[p.id]?.state === 'running'"
                class="task-dot"
                aria-hidden="true"
              />
              <AppIcon v-else-if="setup.results[p.id]?.state === 'done'" name="check" />
              <AppIcon v-else-if="setup.results[p.id]?.state === 'failed'" name="alert" />
              <span class="setup-status-text">{{ statusText(p) }}</span>
            </span>
          </label>
        </section>
      </div>

      <p v-if="setup.error" class="sheet-alert is-error" role="alert">{{ setup.error }}</p>
      <p v-else-if="setup.finished && failures" class="sheet-alert is-warn" role="status">
        {{ failures === 1 ? 'Un programa no se pudo instalar' : `${failures} programas no se pudieron instalar` }}:
        pasa el cursor sobre el mensaje para ver el detalle y vuelve a intentarlo desde Menú →
        Agregar.
      </p>

      <footer class="sheet-foot">
        <span class="sheet-note">
          <template v-if="setup.installing">Instalando {{ progress.done + 1 }} de {{ progress.total }}…</template>
          <template v-else>Puedes volver aquí desde Menú → Agregar.</template>
        </span>
        <template v-if="setup.finished">
          <button ref="mainButton" type="button" class="sheet-btn sheet-btn-main" @click="setup.close()">
            Cerrar
          </button>
        </template>
        <template v-else>
          <button type="button" class="sheet-btn" :disabled="setup.installing" @click="setup.close()">
            Omitir
          </button>
          <button
            ref="mainButton"
            type="button"
            class="sheet-btn sheet-btn-main"
            :disabled="!queue.length || setup.installing"
            @click="start"
          >
            {{ setup.installing ? 'Instalando…' : queue.length ? `Instalar (${queue.length})` : 'Nada que instalar' }}
          </button>
        </template>
      </footer>
    </div>
  </dialog>
</template>
