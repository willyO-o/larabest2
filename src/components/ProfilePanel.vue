<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useSettingsStore } from '../stores/settings'

const settings = useSettingsStore()

const dialog = ref<HTMLDialogElement>()
const input = ref<HTMLInputElement>()
const name = ref('')
const saving = ref(false)
const error = ref<string | null>(null)

const open = computed(() => settings.panel === 'profile')

/** Lo mismo que acepta el núcleo (`profiles::valid_name`). */
const VALID = /^[\p{L}\p{N} ._+-]{1,40}$/u
const valid = computed(() => VALID.test(name.value.trim()))

watch(open, async (value) => {
  if (!value) {
    dialog.value?.close()
    return
  }
  name.value = ''
  error.value = null
  if (!dialog.value?.open) dialog.value?.showModal()
  await nextTick()
  input.value?.focus()
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
  if (!valid.value) return
  saving.value = true
  error.value = null
  try {
    await settings.saveProfile(name.value.trim())
    settings.panel = null
  } catch (e) {
    error.value = (e as { message?: string })?.message ?? String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="profile-title" @cancel="onCancel">
    <form class="sheet-body" @submit.prevent="save">
      <header class="sheet-head">
        <h2 id="profile-title">Guardar perfil</h2>
        <button type="button" class="sheet-close" title="Cerrar" @click="close">
          <AppIcon name="close" />
        </button>
      </header>

      <p class="sheet-text">
        Guarda la pila actual (servidor web, base de datos y versiones de PHP, Node.js, Python y
        los demás) para volver a ella desde Menú → Preferencias → Perfiles.
      </p>
      <label class="field">
        <span class="field-label">Nombre</span>
        <input
          ref="input"
          v-model="name"
          class="field-input"
          maxlength="40"
          autocomplete="off"
          placeholder="Clase PHP 8.3 + MySQL"
          :aria-invalid="!!name && !valid"
        />
        <span class="field-hint" :class="{ 'is-error': !!name && !valid }">
          {{
            name && !valid
              ? 'Letras, números, espacios y -_.+ (hasta 40).'
              : 'Si ya existe uno con ese nombre, se reemplaza.'
          }}
        </span>
      </label>

      <p v-if="error" class="sheet-alert is-error" role="alert">{{ error }}</p>

      <footer class="sheet-foot">
        <button type="button" class="sheet-btn" :disabled="saving" @click="close">Cancelar</button>
        <button type="submit" class="sheet-btn sheet-btn-main" :disabled="!valid || saving">
          Guardar
        </button>
      </footer>
    </form>
  </dialog>
</template>
