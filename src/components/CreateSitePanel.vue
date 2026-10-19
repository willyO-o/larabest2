<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import AppIcon from './AppIcon.vue'
import { attempt } from '../lib/errors'
import { useSettingsStore } from '../stores/settings'
import { useSitesStore } from '../stores/sites'
import type { SitePreset } from '../types/ipc'

const sites = useSitesStore()
const settings = useSettingsStore()

const dbLabel = computed(() => (settings.settings?.database === 'mysql' ? 'MySQL' : 'MariaDB'))

const PRESETS = computed<{ value: SitePreset; label: string; hint: string }[]>(() => [
  { value: 'blank', label: 'En blanco', hint: 'Una carpeta con index.php para empezar desde cero.' },
  {
    value: 'wordpress',
    label: 'WordPress',
    hint: 'Descarga WordPress en español, crea su base de datos y deja wp-config.php listo.',
  },
  {
    value: 'laravel',
    label: 'Laravel',
    hint: `Crea el proyecto con Composer, lo conecta a ${dbLabel.value} y crea las tablas.`,
  },
  {
    value: 'template',
    label: 'Otra…',
    hint: 'Una plantilla de etc/sitios.conf: Symfony, CodeIgniter, Drupal o las tuyas.',
  },
])

const dialog = ref<HTMLDialogElement>()
const input = ref<HTMLInputElement>()
const presetButtons = ref<HTMLButtonElement[]>([])
const name = ref('')
const preset = ref<SitePreset>('blank')
const template = ref<string | null>(null)

const VALID = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?$/

const nameError = computed(() => {
  const value = name.value
  if (!value) return null
  if (!VALID.test(value)) return 'Solo letras, números y guiones; sin espacios ni puntos.'
  if (['adminer', 'phpmyadmin'].includes(value.toLowerCase()))
    return 'Ese nombre está reservado para la base de datos.'
  if (sites.list.some((s) => s.name.toLowerCase() === value.toLowerCase()))
    return 'Ya existe un sitio con ese nombre.'
  return null
})

const url = computed(
  () =>
    `${settings.ssl ? 'https' : 'http'}://${(name.value || 'mi-sitio').toLowerCase()}.${settings.settings?.tld ?? 'test'}`,
)
const currentPreset = computed(() => PRESETS.value.find((p) => p.value === preset.value)!)
const currentTemplate = computed(() => sites.templates.find((t) => t.name === template.value))
const canCreate = computed(
  () =>
    !!name.value &&
    !nameError.value &&
    !sites.creating &&
    (preset.value !== 'template' || !!currentTemplate.value),
)

// Al elegir "Otra…" se preselecciona la primera plantilla.
watch(
  () => [preset.value, sites.templates] as const,
  ([p, list]) => {
    if (p === 'template' && !list.some((t) => t.name === template.value)) {
      template.value = list[0]?.name ?? null
    }
  },
)

const openTemplates = () => attempt(() => invoke('config_open', { target: 'siteTemplates' }))

const creation = computed(() => sites.creation)
const title = computed(() => {
  const c = creation.value
  if (!c) return 'Nuevo sitio'
  if (c.site) return 'Sitio listo'
  if (c.error) return 'No se pudo crear el sitio'
  return `Creando ${c.name}…`
})
const warning = computed(() => (creation.value?.site ? creation.value.warning : null))

watch(
  () => sites.createOpen,
  async (open) => {
    if (!open) {
      dialog.value?.close()
      return
    }
    if (!sites.creation) {
      preset.value = sites.createPreset
      if (sites.createTemplate) template.value = sites.createTemplate
    }
    if (!dialog.value?.open) dialog.value?.showModal()
    await nextTick()
    input.value?.focus()
  },
)

// El menú puede pedir otra plantilla con el formulario ya abierto.
watch(
  () => [sites.createPreset, sites.createTemplate] as const,
  ([value, tpl]) => {
    if (sites.creation) return
    preset.value = value
    if (tpl) template.value = tpl
  },
)

function submit() {
  if (canCreate.value)
    sites.create(name.value, preset.value, preset.value === 'template' ? template.value : null)
}

/** Esc cierra el formulario, pero no mientras se crea el sitio. */
function onCancel(e: Event) {
  e.preventDefault()
  sites.closeCreate()
}

async function onPresetKey(e: KeyboardEvent, index: number) {
  if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
  e.preventDefault()
  const count = PRESETS.value.length
  const next = (index + (e.key === 'ArrowRight' ? 1 : -1) + count) % count
  preset.value = PRESETS.value[next].value
  await nextTick()
  presetButtons.value[next]?.focus()
}

async function backToForm() {
  sites.backToForm()
  await nextTick()
  input.value?.focus()
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="sheet-title" @cancel="onCancel">
    <form v-if="!creation" class="sheet-body" @submit.prevent="submit">
      <header class="sheet-head">
        <h2 id="sheet-title">{{ title }}</h2>
        <button type="button" class="sheet-close" title="Cerrar" @click="sites.closeCreate()">
          <AppIcon name="close" />
        </button>
      </header>

      <label class="field">
        <span class="field-label">Nombre de la carpeta</span>
        <input
          ref="input"
          v-model.trim="name"
          class="field-input"
          :aria-invalid="!!nameError"
          aria-describedby="name-hint"
          autocomplete="off"
          spellcheck="false"
          maxlength="63"
          placeholder="mi-sitio"
        />
        <span id="name-hint" class="field-hint" :class="{ 'is-error': nameError }">
          <template v-if="nameError">{{ nameError }}</template>
          <template v-else>
            www/{{ name || 'mi-sitio' }} → <span class="field-url">{{ url }}</span>
          </template>
        </span>
      </label>

      <div class="field">
        <span id="preset-label" class="field-label">Plantilla</span>
        <div class="segmented segmented-4" role="radiogroup" aria-labelledby="preset-label">
          <button
            v-for="(p, i) in PRESETS"
            :key="p.value"
            ref="presetButtons"
            type="button"
            class="segment"
            role="radio"
            :aria-checked="preset === p.value"
            :tabindex="preset === p.value ? 0 : -1"
            @click="preset = p.value"
            @keydown="onPresetKey($event, i)"
          >
            {{ p.label }}
          </button>
        </div>
        <div v-if="preset === 'template'" class="template-row">
          <select
            v-model="template"
            class="field-input template-select"
            aria-label="Plantilla propia"
            :disabled="!sites.templates.length"
          >
            <option v-for="t in sites.templates" :key="t.name" :value="t.name">
              {{ t.name }}
            </option>
          </select>
          <button type="button" class="sheet-btn" @click="openTemplates">Editar plantillas</button>
        </div>
        <span class="field-hint" :title="currentTemplate?.action">{{
          preset === 'template'
            ? (currentTemplate?.action ?? 'No hay plantillas: agrégalas en etc/sitios.conf.')
            : currentPreset.hint
        }}</span>
      </div>

      <footer class="sheet-foot">
        <button type="button" class="sheet-btn" @click="sites.closeCreate()">Cancelar</button>
        <button type="submit" class="sheet-btn sheet-btn-main" :disabled="!canCreate">
          Crear sitio
        </button>
      </footer>
    </form>

    <div v-else class="sheet-body">
      <header class="sheet-head">
        <h2 id="sheet-title">{{ title }}</h2>
        <button
          v-if="!sites.creating"
          type="button"
          class="sheet-close"
          title="Cerrar"
          @click="sites.closeCreate()"
        >
          <AppIcon name="close" />
        </button>
      </header>

      <ol class="steps" aria-live="polite">
        <li v-for="s in creation.steps" :key="s.step" class="step" :data-state="s.state">
          <span class="step-mark" aria-hidden="true">
            <AppIcon v-if="s.state === 'done'" name="check" />
            <AppIcon v-else-if="s.state === 'failed'" name="close" />
          </span>
          <span class="step-text">{{ s.step }}</span>
        </li>
      </ol>

      <p v-if="creation.error" class="sheet-alert is-error" role="alert">{{ creation.error }}</p>
      <p v-else-if="warning" class="sheet-alert is-warn">{{ warning }}</p>

      <footer class="sheet-foot">
        <template v-if="creation.site">
          <span class="sheet-note">
            <AppIcon v-if="creation.site.url.startsWith('https')" name="lock" />
            {{ creation.site.url }}
          </span>
          <button type="button" class="sheet-btn" @click="sites.openFolder(creation.site.name)">
            Abrir carpeta
          </button>
          <button
            type="button"
            class="sheet-btn sheet-btn-main"
            @click="sites.open(creation.site.name)"
          >
            Abrir sitio
          </button>
        </template>
        <template v-else-if="creation.error">
          <button type="button" class="sheet-btn" @click="sites.closeCreate()">Cerrar</button>
          <button type="button" class="sheet-btn sheet-btn-main" @click="backToForm">
            Volver al formulario
          </button>
        </template>
        <span v-else class="sheet-note">
          {{
            creation.preset === 'laravel' || creation.preset === 'template'
              ? 'Composer puede tardar varios minutos.'
              : 'No cierres la aplicación hasta que termine.'
          }}
        </span>
      </footer>
    </div>
  </dialog>
</template>
