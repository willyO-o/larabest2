<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import AppIcon from './AppIcon.vue'
import { useShareStore } from '../stores/share'

const share = useShareStore()
const dialog = ref<HTMLDialogElement>()
const closeButton = ref<HTMLButtonElement>()

watch(
  () => share.open,
  async (open) => {
    if (!open) {
      dialog.value?.close()
      return
    }
    if (!dialog.value?.open) dialog.value?.showModal()
    await nextTick()
    closeButton.value?.focus()
  },
)

function onCancel(e: Event) {
  e.preventDefault()
  share.close()
}
</script>

<template>
  <dialog ref="dialog" class="sheet" aria-labelledby="share-title" @cancel="onCancel">
    <div class="sheet-body">
      <header class="sheet-head">
        <h2 id="share-title">Compartir en la red local</h2>
        <button
          ref="closeButton"
          type="button"
          class="sheet-close"
          title="Cerrar"
          :disabled="!!share.busy"
          @click="share.close()"
        >
          <AppIcon name="close" />
        </button>
      </header>

      <p class="setup-intro">
        Activa un proyecto y ábrelo desde otro equipo o tu celular conectado a la
        <strong>misma red</strong> (Wi‑Fi o cable), con la dirección que aparece.
      </p>

      <p v-if="share.info && !share.info.ip" class="sheet-alert is-warn" role="status">
        Este equipo no está conectado a ninguna red. Conéctalo al Wi‑Fi o al cable.
      </p>
      <p v-else-if="share.info?.ip" class="share-ip">
        Tu equipo en la red: <span class="field-url">{{ share.info.ip }}</span>
      </p>

      <ul v-if="share.info?.sites.length" class="share-list">
        <li v-for="s in share.info.sites" :key="s.name" class="share-row" :data-shared="s.shared">
          <label class="svc-check share-toggle">
            <input
              type="checkbox"
              :checked="s.shared"
              :disabled="!!share.busy || (!s.shared && !share.info.ip)"
              @change="share.toggle(s.name, ($event.target as HTMLInputElement).checked)"
            />
            <span class="share-name">{{ s.name }}</span>
          </label>
          <span v-if="share.busy === s.name" class="share-state">
            <span class="task-dot" aria-hidden="true" />{{ s.shared ? 'Quitando…' : 'Compartiendo…' }}
          </span>
          <template v-else-if="s.shared && s.url">
            <span class="share-url" :title="s.url">{{ s.url }}</span>
            <span class="share-actions">
              <button type="button" class="row-icon" :title="share.copied === s.url ? 'Copiado' : 'Copiar dirección'" @click="share.copy(s.url)">
                <AppIcon :name="share.copied === s.url ? 'check' : 'copy'" />
              </button>
              <button type="button" class="row-icon" title="Abrir en el navegador" @click="share.openSite(s.name)">
                <AppIcon name="external" />
              </button>
            </span>
          </template>
          <span v-else class="share-state">Solo en este equipo</span>
          <div v-if="s.shared && s.wordpress && !s.wordpressReady" class="share-hint">
            <span>
              WordPress redirige a su dominio .test, que solo existe en este equipo. Ajusta su
              wp-config.php para que, abierto desde la red, use esa dirección (se guarda una copia).
            </span>
            <button
              type="button"
              class="sheet-btn share-fix"
              :disabled="!!share.busy"
              @click="share.fixWordpress(s.name)"
            >
              Ajustar wp-config.php
            </button>
          </div>
        </li>
      </ul>
      <p v-else-if="share.info" class="db-list-note">
        Todavía no hay proyectos: crea una carpeta en www o usa Nuevo sitio.
      </p>

      <p v-if="share.error" class="sheet-alert is-error" role="alert">{{ share.error }}</p>
      <p v-else-if="share.shared.length && share.info && !share.info.webRunning" class="sheet-alert is-warn">
        El servidor web está detenido: inícialo para que se vea.
      </p>

      <footer class="sheet-foot">
        <span class="sheet-note">
          Solo los equipos de tu red pueden entrar. Desactívalo al terminar.
        </span>
        <button type="button" class="sheet-btn sheet-btn-main" :disabled="!!share.busy" @click="share.close()">
          Listo
        </button>
      </footer>
    </div>
  </dialog>
</template>
