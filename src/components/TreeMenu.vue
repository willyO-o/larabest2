<script setup lang="ts">
import { computed, ref } from 'vue'
import AppIcon from './AppIcon.vue'
import { useServicesStore } from '../stores/services'
import { useSettingsStore } from '../stores/settings'
import { useSitesStore } from '../stores/sites'

defineEmits<{ close: [] }>()

const services = useServicesStore()
const sites = useSitesStore()
const settings = useSettingsStore()

type Node = 'sites' | 'php' | 'nginx' | 'mariadb' | 'logs' | 'prefs'
const open = ref<Set<Node>>(new Set(['sites']))

function toggle(node: Node) {
  const next = new Set(open.value)
  if (next.has(node)) next.delete(node)
  else next.add(node)
  open.value = next
  if (node === 'php' && next.has('php')) settings.loadPhpVersions()
}

const activePhp = computed(() => services.byId.php?.version ?? null)
const nginxUp = computed(() => services.byId.nginx?.state === 'running')
const mariadbUp = computed(() => services.byId.mariadb?.state === 'running')
</script>

<template>
  <nav class="tree" aria-label="Menú">
    <header class="tree-head">
      <span>Menú</span>
      <button class="icon-btn" title="Cerrar" @click="$emit('close')">
        <AppIcon name="close" />
      </button>
    </header>

    <ul class="tree-root">
      <!-- Sitios -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('sites')" @click="toggle('sites')">
          <AppIcon name="chevron" class="chev" />Sitios
          <span class="count">{{ sites.list.length }}</span>
        </button>
        <ul v-if="open.has('sites')" class="tree-children">
          <li v-if="sites.list.length === 0" class="tree-empty">
            Crea una carpeta dentro de www/ y aparecerá aquí como &lt;carpeta&gt;.test
          </li>
          <li v-for="site in sites.list" :key="site.name" class="tree-site">
            <button class="tree-leaf" :title="site.documentRoot" @click="sites.open(site.name)">
              <span class="site-domain">{{ site.domain }}</span>
              <AppIcon v-if="!site.inHosts" name="alert" class="warn" />
            </button>
            <button class="icon-btn" title="Abrir carpeta" @click="sites.openFolder(site.name)">
              <AppIcon name="folder" />
            </button>
            <button class="icon-btn" title="Terminal aquí" @click="sites.openTerminal(site.path)">
              <AppIcon name="terminal" />
            </button>
          </li>
          <li v-if="sites.missingHosts.length" class="tree-note">
            <span>{{ sites.missingHosts.length === 1 ? 'Falta 1 dominio' : `Faltan ${sites.missingHosts.length} dominios` }} en el archivo hosts.</span>
            <button class="link" :disabled="sites.syncing" @click="sites.sync()">
              Reintentar
            </button>
          </li>
        </ul>
      </li>

      <!-- PHP -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('php')" @click="toggle('php')">
          <AppIcon name="chevron" class="chev" />PHP
          <span class="count">{{ activePhp ?? '—' }}</span>
        </button>
        <ul v-if="open.has('php')" class="tree-children">
          <li class="tree-label">Versión</li>
          <li v-if="settings.phpVersions.length === 0" class="tree-empty">
            Copia una versión de PHP en bin/php/&lt;versión&gt;/
          </li>
          <li v-for="v in settings.phpVersions" :key="v.version">
            <button
              class="tree-leaf radio"
              :aria-checked="v.version === activePhp"
              role="menuitemradio"
              :disabled="services.pending.has('php')"
              @click="v.version !== activePhp && settings.switchPhp(v.version)"
            >
              <span class="radio-dot" />{{ v.version }}
            </button>
          </li>
          <li class="tree-leaf is-later">php.ini <small>fase 2</small></li>
          <li class="tree-leaf is-later">Extensiones <small>fase 2</small></li>
        </ul>
      </li>

      <!-- Nginx -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('nginx')" @click="toggle('nginx')">
          <AppIcon name="chevron" class="chev" />Nginx
        </button>
        <ul v-if="open.has('nginx')" class="tree-children">
          <li>
            <button class="tree-leaf" :disabled="!nginxUp" @click="services.reload('nginx')">
              Recargar configuración
            </button>
          </li>
        </ul>
      </li>

      <!-- MariaDB -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('mariadb')" @click="toggle('mariadb')">
          <AppIcon name="chevron" class="chev" />MariaDB
        </button>
        <ul v-if="open.has('mariadb')" class="tree-children">
          <li>
            <button class="tree-leaf" :disabled="!mariadbUp" @click="settings.openDatabase()">
              Abrir Adminer
            </button>
          </li>
          <li class="tree-hint">Usuario root sin contraseña · 127.0.0.1:{{ services.byId.mariadb?.port ?? 3306 }}</li>
        </ul>
      </li>

      <!-- Logs -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('logs')" @click="toggle('logs')">
          <AppIcon name="chevron" class="chev" />Logs
        </button>
        <ul v-if="open.has('logs')" class="tree-children">
          <li class="tree-leaf is-later">Ver logs <small>fase 2</small></li>
        </ul>
      </li>

      <!-- Preferencias -->
      <li>
        <button class="tree-node" :aria-expanded="open.has('prefs')" @click="toggle('prefs')">
          <AppIcon name="chevron" class="chev" />Preferencias
        </button>
        <ul v-if="open.has('prefs')" class="tree-children">
          <li class="tree-label">Carpeta raíz</li>
          <li class="tree-path">{{ settings.settings?.root }}</li>
          <li>
            <button class="tree-leaf" @click="settings.chooseRoot()">Cambiar carpeta…</button>
          </li>
          <li v-if="settings.pendingRoot" class="tree-note">
            Se usará {{ settings.pendingRoot }} al reiniciar la aplicación.
          </li>
        </ul>
      </li>
    </ul>
  </nav>
</template>
