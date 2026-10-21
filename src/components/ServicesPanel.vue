<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useServicesPanelStore } from '../stores/servicesPanel'
import { SERVICE_LABEL, useServicesStore } from '../stores/services'
import { useSitesStore } from '../stores/sites'
import type { PortSettings, ServiceId, WebServer } from '../types/ipc'

const panel = useServicesPanelStore()
const services = useServicesStore()
const sites = useSitesStore()

type PortKey = keyof PortSettings

interface PortField {
  key: PortKey
  /** Texto corto junto al campo cuando la fila tiene dos puertos. */
  caption?: string
}

interface Row {
  id: ServiceId
  /** MariaDB y los opcionales se pueden quitar de la pila. */
  toggle: boolean
  ports: PortField[]
}

const ROWS: Row[] = [
  { id: 'php', toggle: false, ports: [{ key: 'php', caption: 'FastCGI' }] },
  { id: 'mariadb', toggle: true, ports: [{ key: 'mariadb' }] },
  { id: 'postgresql', toggle: true, ports: [{ key: 'postgresql' }] },
  { id: 'redis', toggle: true, ports: [{ key: 'redis' }] },
  { id: 'mongodb', toggle: true, ports: [{ key: 'mongodb' }] },
  {
    id: 'mailpit',
    toggle: true,
    ports: [
      { key: 'mailpitSmtp', caption: 'SMTP' },
      { key: 'mailpitHttp', caption: 'Web' },
    ],
  },
]

const WEB_SERVERS: WebServer[] = ['nginx', 'apache']

/** Quién usa cada puerto, para los mensajes. */
const OWNER: Record<PortKey, string> = {
  http: 'HTTP',
  https: 'HTTPS',
  php: 'PHP',
  mariadb: 'MariaDB',
  postgresql: 'PostgreSQL',
  redis: 'Redis',
  mongodb: 'MongoDB',
  mailpitSmtp: 'Mailpit (SMTP)',
  mailpitHttp: 'Mailpit (web)',
}

/** Servicio al que pertenece cada puerto (el web, según el elegido). */
const SERVICE_OF: Record<PortKey, ServiceId | 'web'> = {
  http: 'web',
  https: 'web',
  php: 'php',
  mariadb: 'mariadb',
  postgresql: 'postgresql',
  redis: 'redis',
  mongodb: 'mongodb',
  mailpitSmtp: 'mailpit',
  mailpitHttp: 'mailpit',
}

const dialog = ref<HTMLDialogElement>()
const firstInput = ref<HTMLInputElement>()
const webServer = ref<WebServer>('nginx')
const enabled = reactive<Partial<Record<ServiceId, boolean>>>({})
const text = reactive<Record<PortKey, string>>({
  http: '',
  https: '',
  php: '',
  mariadb: '',
  postgresql: '',
  redis: '',
  mongodb: '',
  mailpitSmtp: '',
  mailpitHttp: '',
})

const phpCount = computed(() => panel.current?.phpPortCount ?? 1)

function installed(id: ServiceId) {
  return !!services.byId[id]?.version
}

function version(id: ServiceId) {
  return services.byId[id]?.version ?? 'sin instalar'
}

/** Carga el formulario con lo guardado cada vez que se abre. */
function load() {
  const current = panel.current
  if (!current) return
  webServer.value = current.webServer
  for (const key of Object.keys(text) as PortKey[]) text[key] = String(current.ports[key])
  for (const row of ROWS) {
    if (row.toggle) enabled[row.id] = services.byId[row.id]?.enabled ?? false
  }
}

watch(
  () => panel.open,
  async (open) => {
    if (!open) {
      dialog.value?.close()
      return
    }
    load()
    if (!dialog.value?.open) dialog.value?.showModal()
    await nextTick()
    firstInput.value?.focus()
    firstInput.value?.select()
  },
)

function parse(value: string): number | null {
  if (!/^\d{1,5}$/.test(value.trim())) return null
  const n = Number(value)
  return n >= 1 && n <= 65535 ? n : null
}

/** Puertos que ocupa cada campo (PHP usa varios seguidos en Windows). */
function portsOf(key: PortKey): number[] {
  const n = parse(text[key])
  if (n === null) return []
  if (key !== 'php') return [n]
  return Array.from({ length: phpCount.value }, (_, i) => n + i)
}

/** Primer problema de cada campo: no es un número válido o choca con otro. */
const problems = computed(() => {
  const out: Partial<Record<PortKey, string>> = {}
  const keys = Object.keys(text) as PortKey[]
  for (const key of keys) {
    if (parse(text[key]) === null) {
      out[key] = `El puerto de ${OWNER[key]} debe ser un número entre 1 y 65535.`
      continue
    }
    if (portsOf(key).some((p) => p > 65535)) {
      out[key] = `Los ${phpCount.value} puertos de PHP se pasan de 65535.`
      continue
    }
    const mine = portsOf(key)
    const other = keys.find((k) => k !== key && portsOf(k).some((p) => mine.includes(p)))
    if (other) {
      const port = portsOf(other).find((p) => mine.includes(p))
      out[key] = `El puerto ${port} está repetido: lo usarían ${OWNER[key]} y ${OWNER[other]}.`
    }
  }
  return out
})

const firstProblem = computed(() => Object.values(problems.value)[0] ?? null)

const ports = computed<PortSettings | null>(() => {
  if (firstProblem.value) return null
  const out = {} as PortSettings
  for (const key of Object.keys(text) as PortKey[]) out[key] = parse(text[key])!
  return out
})

// Avisa de puertos ocupados por otro programa mientras se escribe.
let timer: ReturnType<typeof setTimeout> | undefined
watch(
  [ports, () => panel.open],
  ([value, open]) => {
    clearTimeout(timer)
    if (!open || !value) return
    timer = setTimeout(() => panel.checkBusy(value), 350)
  },
  { deep: true },
)

function inStack(key: PortKey) {
  const service = SERVICE_OF[key]
  if (service === 'web' || service === 'php') return true
  return !!enabled[service]
}

/** Puerto ocupado por otro programa en un servicio que va a arrancar. */
function busyOf(key: PortKey) {
  if (!inStack(key)) return null
  const mine = portsOf(key)
  return panel.busy.find((b) => mine.includes(b.port)) ?? null
}

const busyWarnings = computed(() =>
  (Object.keys(text) as PortKey[]).flatMap((key) => {
    const b = busyOf(key)
    return b ? [`${OWNER[key]} (${b.port}): lo ocupa ${b.program}.`] : []
  }),
)

/** Los sitios creados guardan estos puertos en wp-config.php o .env. */
const sitesWarning = computed(() => {
  const saved = panel.current?.ports
  if (!saved || !sites.list.length) return null
  const changed = (['mariadb', 'redis', 'mailpitSmtp'] as PortKey[]).filter(
    (k) => parse(text[k]) !== null && parse(text[k]) !== saved[k],
  )
  if (!changed.length) return null
  return `Los sitios ya creados guardan el puerto de ${changed
    .map((k) => OWNER[k])
    .join(' y ')} en wp-config.php o .env: cámbialo también ahí.`
})

/** "9000–9003" cuando PHP usa varios puertos (Windows). */
function phpRange() {
  const n = parse(text.php)
  return n !== null && phpCount.value > 1 ? `${n}–${n + phpCount.value - 1}` : ''
}

function submit() {
  if (!ports.value || panel.saving) return
  panel.save({
    webServer: webServer.value,
    enabled: ROWS.filter((r) => r.toggle).map((r) => ({
      id: r.id,
      enabled: !!enabled[r.id],
    })),
    ports: ports.value,
  })
}

function onCancel(e: Event) {
  e.preventDefault()
  panel.close()
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="services-title" @cancel="onCancel">
    <form class="sheet-body" @submit.prevent="submit">
      <header class="sheet-head">
        <h2 id="services-title">Servicios y puertos</h2>
        <button type="button" class="sheet-close" title="Cerrar" @click="panel.close()">
          <AppIcon name="close" />
        </button>
      </header>

      <div class="svc-table">
        <div class="svc-row">
          <span class="svc-name">Servidor web</span>
          <div class="svc-detail">
            <div class="segmented svc-web" role="radiogroup" aria-label="Servidor web">
              <button
                v-for="w in WEB_SERVERS"
                :key="w"
                type="button"
                class="segment"
                role="radio"
                :aria-checked="webServer === w"
                :disabled="!installed(w)"
                :title="installed(w) ? version(w) : 'Agrégalo desde Menú → Agregar'"
                @click="webServer = w"
              >
                {{ SERVICE_LABEL[w] }}
              </button>
            </div>
          </div>
          <div class="svc-ports">
            <label class="port">
              <span class="port-caption">HTTP</span>
              <input
                ref="firstInput"
                v-model="text.http"
                class="field-input port-input"
                inputmode="numeric"
                maxlength="5"
                aria-label="Puerto HTTP"
                :aria-invalid="!!problems.http"
                :data-busy="!!busyOf('http')"
              />
            </label>
            <label class="port">
              <span class="port-caption">HTTPS</span>
              <input
                v-model="text.https"
                class="field-input port-input"
                inputmode="numeric"
                maxlength="5"
                aria-label="Puerto HTTPS"
                :aria-invalid="!!problems.https"
                :data-busy="!!busyOf('https')"
              />
            </label>
          </div>
        </div>

        <div v-for="row in ROWS" :key="row.id" class="svc-row" :data-off="row.toggle && !enabled[row.id]">
          <label v-if="row.toggle" class="svc-name svc-check" :title="installed(row.id) ? '' : 'Agrégalo desde Menú → Agregar'">
            <input
              v-model="enabled[row.id]"
              type="checkbox"
              :disabled="!installed(row.id) && !enabled[row.id]"
            />
            <span>{{ SERVICE_LABEL[row.id] }}</span>
          </label>
          <span v-else class="svc-name">{{ SERVICE_LABEL[row.id] }}</span>
          <span class="svc-detail svc-version" :class="{ 'is-missing': !installed(row.id) }">
            {{ version(row.id) }}<template v-if="!row.toggle"> · siempre activo</template
            ><template v-if="row.id === 'php' && phpRange()"> · usa {{ phpRange() }}</template>
          </span>
          <div class="svc-ports">
            <label v-for="f in row.ports" :key="f.key" class="port">
              <span v-if="f.caption" class="port-caption">{{ f.caption }}</span>
              <input
                v-model="text[f.key]"
                class="field-input port-input"
                inputmode="numeric"
                maxlength="5"
                :aria-label="`Puerto de ${OWNER[f.key]}`"
                :aria-invalid="!!problems[f.key]"
                :data-busy="!!busyOf(f.key)"
              />
            </label>
          </div>
        </div>
      </div>

      <p v-if="panel.error" class="sheet-alert is-error" role="alert">{{ panel.error }}</p>
      <p v-else-if="firstProblem" class="sheet-alert is-error" role="alert">{{ firstProblem }}</p>
      <p v-else-if="busyWarnings.length || sitesWarning" class="sheet-alert is-warn">
        <template v-for="w in busyWarnings" :key="w">{{ w }}<br /></template>
        <template v-if="busyWarnings.length">Cierra ese programa o cambia el puerto.<br v-if="sitesWarning" /></template>
        <template v-if="sitesWarning">{{ sitesWarning }}</template>
      </p>

      <footer class="sheet-foot">
        <span class="sheet-note">Los servicios en marcha se reinician solos.</span>
        <button type="button" class="sheet-btn" :disabled="panel.saving" @click="panel.close()">
          Cancelar
        </button>
        <button type="submit" class="sheet-btn sheet-btn-main" :disabled="!ports || panel.saving">
          {{ panel.saving ? 'Guardando…' : 'Guardar' }}
        </button>
      </footer>
    </form>
  </dialog>
</template>
