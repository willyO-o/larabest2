<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useSettingsStore } from '../stores/settings'
import type { PortSettings } from '../types/ipc'

const settings = useSettingsStore()

const dialog = ref<HTMLDialogElement>()
const first = ref<HTMLInputElement[]>([])
const form = ref<PortSettings | null>(null)
const saving = ref(false)
const error = ref<string | null>(null)

const open = computed(() => settings.panel === 'ports')
const dbLabel = computed(() => (settings.settings?.database === 'mysql' ? 'MySQL' : 'MariaDB'))

type PortKey = Exclude<keyof PortSettings, 'tld'>

const FIELDS = computed<{ key: PortKey; label: string }[]>(() => [
  { key: 'http', label: 'HTTP' },
  { key: 'https', label: 'HTTPS' },
  { key: 'php', label: 'PHP (primero)' },
  { key: 'database', label: dbLabel.value },
  { key: 'postgresql', label: 'PostgreSQL' },
  { key: 'redis', label: 'Redis' },
  { key: 'mongodb', label: 'MongoDB' },
  { key: 'mailpitSmtp', label: 'Mailpit SMTP' },
  { key: 'mailpitHttp', label: 'Mailpit web' },
])

const TLD = /^[a-z][a-z0-9-]{1,23}$/

const tldError = computed(() => {
  const tld = form.value?.tld ?? ''
  if (!TLD.test(tld)) return 'Solo minúsculas, números y guiones.'
  if (['dev', 'app', 'localhost'].includes(tld)) return 'Los navegadores lo tratan distinto.'
  return null
})

const portError = (value: number) =>
  !Number.isInteger(value) || value < 1 || value > 65535 ? 'De 1 a 65535' : null

const valid = computed(
  () => !!form.value && !tldError.value && FIELDS.value.every((f) => !portError(form.value![f.key])),
)

watch(open, async (value) => {
  if (!value) {
    dialog.value?.close()
    return
  }
  error.value = null
  try {
    form.value = await settings.getPorts()
  } catch (e) {
    error.value = (e as { message?: string })?.message ?? String(e)
  }
  if (!dialog.value?.open) dialog.value?.showModal()
  await nextTick()
  first.value[0]?.focus()
})

function close() {
  if (saving.value) return
  settings.panel = null
}

function onCancel(e: Event) {
  e.preventDefault()
  close()
}

async function save() {
  if (!form.value || !valid.value) return
  saving.value = true
  error.value = null
  try {
    await settings.savePorts({ ...form.value })
    settings.panel = null
  } catch (e) {
    error.value = (e as { message?: string })?.message ?? String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="ports-title" @cancel="onCancel">
    <form class="sheet-body" @submit.prevent="save">
      <header class="sheet-head">
        <h2 id="ports-title">Puertos y dominio</h2>
        <button type="button" class="sheet-close" title="Cerrar" @click="close">
          <AppIcon name="close" />
        </button>
      </header>

      <div v-if="form" class="ports-grid">
        <label v-for="f in FIELDS" :key="f.key" class="field field-compact">
          <span class="field-label">{{ f.label }}</span>
          <input
            ref="first"
            v-model.number="form[f.key]"
            class="field-input"
            type="number"
            min="1"
            max="65535"
            inputmode="numeric"
            :aria-invalid="!!portError(form[f.key])"
          />
        </label>
        <label class="field field-compact">
          <span class="field-label">Dominio</span>
          <span class="tld-input">
            <span class="tld-prefix" aria-hidden="true">sitio.</span>
            <input
              v-model.trim="form.tld"
              class="field-input"
              spellcheck="false"
              autocomplete="off"
              maxlength="24"
              :aria-invalid="!!tldError"
              :title="tldError ?? ''"
            />
          </span>
        </label>
      </div>

      <p v-if="error" class="sheet-alert is-error" role="alert">{{ error }}</p>

      <footer class="sheet-foot">
        <span class="sheet-note">
          {{ tldError ?? 'Lo que esté en marcha y cambie se reinicia solo.' }}
        </span>
        <button type="button" class="sheet-btn" :disabled="saving" @click="close">Cancelar</button>
        <button type="submit" class="sheet-btn sheet-btn-main" :disabled="!valid || saving">
          {{ saving ? 'Guardando…' : 'Guardar' }}
        </button>
      </footer>
    </form>
  </dialog>
</template>
