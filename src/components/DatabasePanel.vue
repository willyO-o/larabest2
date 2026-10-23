<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useDatabaseStore } from '../stores/database'
import { SERVICE_LABEL, useServicesStore } from '../stores/services'
import type { DatabaseMode } from '../types/ipc'

const db = useDatabaseStore()
const services = useServicesStore()

const TABS: { value: DatabaseMode; label: string }[] = [
  { value: 'create', label: 'Nueva base' },
  { value: 'backup', label: 'Respaldar' },
  { value: 'password', label: 'Contraseña de root' },
]

/** Lo mismo que acepta el núcleo (`domain::database`). */
const PASSWORD_SYMBOLS = '-_.@!*+=:,~^'
const SYSTEM = ['information_schema', 'performance_schema', 'mysql', 'sys']

const dialog = ref<HTMLDialogElement>()
const firstField = ref<HTMLInputElement | HTMLSelectElement>()
const startButton = ref<HTMLButtonElement>()
const name = ref('')
const chosen = ref('')
const password = ref('')
const confirm = ref('')
const reveal = ref(false)

/** MariaDB o MySQL: la elegida (la que está en la pila). */
const engine = computed(
  () => db.info?.engine ?? (services.byId.mysql?.enabled ? 'mysql' : 'mariadb'),
)
const engineLabel = computed(() => SERVICE_LABEL[engine.value])
const server = computed(() => services.byId[engine.value])
const running = computed(() => server.value?.state === 'running')
const installed = computed(() => !!server.value?.version)
const starting = computed(() => server.value?.state === 'starting')

/** Nombre tal como se creará: minúsculas y `_` en lugar de `-`. */
const normalized = computed(() => name.value.trim().toLowerCase().replace(/-/g, '_'))

const nameError = computed(() => {
  const n = normalized.value
  if (!n) return null
  if (n.length > 64) return 'El nombre puede tener hasta 64 caracteres.'
  if (!/^[a-z0-9_]+$/.test(n)) return 'Usa solo letras sin tilde, números, guiones y guiones bajos.'
  if (/^\d+$/.test(n)) return 'El nombre no puede ser solo números.'
  if (SYSTEM.includes(n)) return `${n} es una base del sistema.`
  if (db.info?.databases.includes(n)) return `Ya existe una base de datos llamada ${n}.`
  return null
})

const passwordError = computed(() => {
  const p = password.value
  if (p.length > 64) return 'La contraseña puede tener hasta 64 caracteres.'
  const bad = [...p].find((c) => !/[A-Za-z0-9]/.test(c) && !PASSWORD_SYMBOLS.includes(c))
  if (bad !== undefined) {
    return `La contraseña no puede llevar ${bad === ' ' ? 'espacios' : `«${bad}»`}.`
  }
  if (confirm.value && confirm.value !== p) return 'Las dos contraseñas no coinciden.'
  return null
})

const canSubmit = computed(() => {
  if (!running.value || db.busy) return false
  switch (db.mode) {
    case 'create':
      return !!normalized.value && !nameError.value
    case 'backup':
      return !!chosen.value
    default:
      return !passwordError.value && confirm.value === password.value &&
        (!!password.value || !!db.info?.hasPassword)
  }
})

const submitLabel = computed(() => {
  if (db.busy) {
    return { create: 'Creando…', backup: 'Respaldando…', password: 'Cambiando…' }[db.mode]
  }
  if (db.mode === 'create') return 'Crear base de datos'
  if (db.mode === 'backup') return 'Respaldar'
  return !password.value && db.info?.hasPassword ? 'Quitar contraseña' : 'Cambiar contraseña'
})

function reset() {
  name.value = ''
  password.value = ''
  confirm.value = ''
  reveal.value = false
  chosen.value = db.info?.databases[0] ?? ''
}

async function focusFirst() {
  await nextTick()
  ;(firstField.value ?? startButton.value)?.focus()
}

watch(
  () => db.open,
  async (open) => {
    if (!open) {
      dialog.value?.close()
      return
    }
    reset()
    if (!dialog.value?.open) dialog.value?.showModal()
    focusFirst()
  },
)

watch(() => db.mode, focusFirst)

// Al llegar la lista, se elige la primera base para respaldar.
watch(
  () => db.info?.databases,
  (list) => {
    if (list && !list.includes(chosen.value)) chosen.value = list[0] ?? ''
  },
)

// Si MariaDB arranca con el panel abierto, se cargan sus bases.
watch(running, (now, before) => {
  if (db.open && now && !before) db.refresh()
})

async function submit() {
  if (!canSubmit.value) return
  if (db.mode === 'create') {
    if (await db.create(name.value)) name.value = ''
  } else if (db.mode === 'backup') {
    await db.backup(chosen.value)
  } else if (await db.setPassword(password.value)) {
    password.value = ''
    confirm.value = ''
  }
}

function onCancel(e: Event) {
  e.preventDefault()
  db.close()
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="db-title" @cancel="onCancel">
    <form class="sheet-body" @submit.prevent="submit">
      <header class="sheet-head">
        <h2 id="db-title">Bases de datos de {{ engineLabel }}</h2>
        <button type="button" class="sheet-close" title="Cerrar" :disabled="db.busy" @click="db.close()">
          <AppIcon name="close" />
        </button>
      </header>

      <div class="segmented db-tabs" role="tablist" aria-label="Acción">
        <button
          v-for="t in TABS"
          :key="t.value"
          type="button"
          class="segment"
          role="tab"
          :aria-selected="db.mode === t.value"
          :aria-checked="db.mode === t.value"
          :disabled="db.busy"
          @click="db.setMode(t.value)"
        >
          {{ t.label }}
        </button>
      </div>

      <!-- Sin MariaDB en marcha no se puede hacer nada: se ofrece iniciarlo. -->
      <div v-if="!running" class="db-off" role="status">
        <AppIcon name="database" />
        <p>
          <template v-if="!installed">{{ engineLabel }} no está instalado. Agrégalo desde Menú → Agregar.</template>
          <template v-else-if="starting">Iniciando {{ engineLabel }}…</template>
          <template v-else>{{ engineLabel }} está detenido: hay que iniciarlo para trabajar con las bases.</template>
        </p>
        <button
          v-if="installed"
          ref="startButton"
          type="button"
          class="sheet-btn sheet-btn-main"
          :disabled="starting"
          @click="services.start(engine)"
        >
          Iniciar {{ engineLabel }}
        </button>
      </div>

      <template v-else-if="db.mode === 'create'">
        <label class="field">
          <span class="field-label">Nombre</span>
          <input
            ref="firstField"
            v-model="name"
            class="field-input"
            maxlength="64"
            spellcheck="false"
            autocapitalize="off"
            placeholder="mi_tienda"
            :aria-invalid="!!nameError"
            aria-describedby="db-name-hint"
          />
          <span id="db-name-hint" class="field-hint" :class="{ 'is-error': nameError }">
            <template v-if="nameError">{{ nameError }}</template>
            <template v-else-if="normalized && normalized !== name.trim()">
              Se creará como <strong>{{ normalized }}</strong> (utf8mb4).
            </template>
            <template v-else>Vacía, en utf8mb4. Usuario root en 127.0.0.1.</template>
          </span>
        </label>
        <p v-if="db.info?.databases.length" class="db-list-note">
          Ya existen: {{ db.info.databases.join(', ') }}
        </p>
      </template>

      <template v-else-if="db.mode === 'backup'">
        <label v-if="db.info?.databases.length" class="field">
          <span class="field-label">Base de datos</span>
          <select ref="firstField" v-model="chosen" class="field-input">
            <option v-for="d in db.info.databases" :key="d" :value="d">{{ d }}</option>
          </select>
          <span class="field-hint">
            Un archivo .sql con tablas, datos, rutinas y triggers en
            <span class="field-url">{{ db.info.backupsDir }}</span>
          </span>
        </label>
        <p v-else-if="!db.loading" class="db-list-note">Todavía no hay bases de datos para respaldar.</p>
      </template>

      <template v-else>
        <p class="db-list-note">
          <template v-if="db.info?.hasPassword">
            Root tiene contraseña. Deja los campos vacíos para quitarla.
          </template>
          <template v-else>Root no tiene contraseña (como recién instalado).</template>
        </p>
        <label class="field">
          <span class="field-label">Contraseña nueva</span>
          <input
            ref="firstField"
            v-model="password"
            class="field-input"
            :type="reveal ? 'text' : 'password'"
            maxlength="64"
            autocomplete="new-password"
            spellcheck="false"
            :aria-invalid="!!passwordError && !!password"
          />
        </label>
        <label class="field">
          <span class="field-label">Repítela</span>
          <input
            v-model="confirm"
            class="field-input"
            :type="reveal ? 'text' : 'password'"
            maxlength="64"
            autocomplete="new-password"
            spellcheck="false"
            :aria-invalid="!!confirm && confirm !== password"
          />
          <span class="field-hint" :class="{ 'is-error': passwordError }">
            {{ passwordError ?? `Letras, números y ${PASSWORD_SYMBOLS}` }}
          </span>
        </label>
        <label class="svc-check db-reveal">
          <input v-model="reveal" type="checkbox" />
          <span>Mostrar</span>
        </label>
      </template>

      <p v-if="db.error" class="sheet-alert is-error" role="alert">{{ db.error }}</p>
      <p v-else-if="db.done" class="sheet-alert is-ok" role="status">{{ db.done }}</p>
      <p v-else-if="db.mode === 'password' && running" class="sheet-alert is-warn">
        Los sitios ya creados guardan la contraseña en su .env o wp-config.php: cámbiala también ahí.
      </p>

      <footer class="sheet-foot">
        <button
          v-if="db.mode === 'backup'"
          type="button"
          class="sheet-btn db-folder"
          @click="db.openBackups()"
        >
          <AppIcon name="folder" />Carpeta de respaldos
        </button>
        <span v-else class="sheet-note">Solo escucha en 127.0.0.1.</span>
        <button type="button" class="sheet-btn" :disabled="db.busy" @click="db.close()">Cerrar</button>
        <button v-if="running" type="submit" class="sheet-btn sheet-btn-main" :disabled="!canSubmit">
          {{ submitLabel }}
        </button>
      </footer>
    </form>
  </dialog>
</template>
