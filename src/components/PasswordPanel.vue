<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useSettingsStore } from '../stores/settings'

const settings = useSettingsStore()

const dialog = ref<HTMLDialogElement>()
const input = ref<HTMLInputElement>()
const password = ref('')
const repeat = ref('')
const show = ref(false)
const saving = ref(false)
const error = ref<string | null>(null)
const done = ref<string | null>(null)

const open = computed(() => settings.panel === 'password')
const dbLabel = computed(() => (settings.settings?.database === 'mysql' ? 'MySQL' : 'MariaDB'))

/** Lo mismo que acepta el núcleo (`mariadb::valid_password`). */
const ALLOWED = /^[A-Za-z0-9_\-.@!%*+=?,:]{0,64}$/

const problem = computed(() => {
  if (!ALLOWED.test(password.value))
    return 'Solo letras, números y _-.@!%*+=?,: (sin espacios ni comillas).'
  if (password.value !== repeat.value) return 'Las dos contraseñas no coinciden.'
  return null
})

/** Vacía y sin contraseña: no hay nada que quitar. */
const nothingToDo = computed(() => !password.value && !settings.settings?.hasRootPassword)

watch(open, async (value) => {
  if (!value) {
    dialog.value?.close()
    return
  }
  password.value = ''
  repeat.value = ''
  error.value = null
  done.value = null
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
  if (problem.value) return
  saving.value = true
  error.value = null
  try {
    done.value = await settings.saveRootPassword(password.value)
  } catch (e) {
    error.value = (e as { message?: string })?.message ?? String(e)
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="password-title" @cancel="onCancel">
    <form class="sheet-body" @submit.prevent="save">
      <header class="sheet-head">
        <h2 id="password-title">Contraseña de root de {{ dbLabel }}</h2>
        <button type="button" class="sheet-close" title="Cerrar" @click="close">
          <AppIcon name="close" />
        </button>
      </header>

      <template v-if="!done">
        <p class="sheet-text">
          {{ dbLabel }} solo escucha en 127.0.0.1, así que root puede quedar sin contraseña. Si le
          pones una, {{ settings.settings?.appName ?? 'la app' }} la usa para detenerlo, crear bases
          de datos y entrar a Adminer y phpMyAdmin. Déjala vacía para quitarla.
        </p>
        <div class="password-grid">
          <label class="field field-compact">
            <span class="field-label">Contraseña nueva</span>
            <input
              ref="input"
              v-model="password"
              class="field-input"
              :type="show ? 'text' : 'password'"
              autocomplete="new-password"
              maxlength="64"
              :aria-invalid="!ALLOWED.test(password)"
            />
          </label>
          <label class="field field-compact">
            <span class="field-label">Repítela</span>
            <input
              v-model="repeat"
              class="field-input"
              :type="show ? 'text' : 'password'"
              autocomplete="new-password"
              maxlength="64"
            />
          </label>
        </div>
        <label class="check">
          <input v-model="show" type="checkbox" />
          <span>Mostrar</span>
        </label>
      </template>

      <p v-if="done" class="sheet-alert is-ok" role="status">{{ done }}</p>
      <p v-if="error" class="sheet-alert is-error" role="alert">{{ error }}</p>

      <footer class="sheet-foot">
        <template v-if="done">
          <button type="button" class="sheet-btn sheet-btn-main" @click="close">Listo</button>
        </template>
        <template v-else>
          <span class="sheet-note">{{
            problem ??
            (settings.settings?.hasRootPassword ? 'Ahora tiene contraseña.' : 'Ahora no tiene contraseña.')
          }}</span>
          <button type="button" class="sheet-btn" :disabled="saving" @click="close">Cancelar</button>
          <button type="submit" class="sheet-btn sheet-btn-main" :disabled="!!problem || saving || nothingToDo">
            {{ saving ? 'Cambiando…' : password ? 'Cambiar' : 'Quitar contraseña' }}
          </button>
        </template>
      </footer>
    </form>
  </dialog>
</template>
