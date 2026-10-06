<script setup lang="ts">
import { nextTick, ref } from 'vue'
import AppIcon, { type IconName } from './AppIcon.vue'
import { useSettingsStore } from '../stores/settings'
import type { ThemePreference } from '../types/ipc'

const settings = useSettingsStore()

const OPTIONS: { value: ThemePreference; label: string; icon: IconName }[] = [
  { value: 'system', label: 'Tema del sistema', icon: 'monitor' },
  { value: 'light', label: 'Tema claro', icon: 'sun' },
  { value: 'dark', label: 'Tema oscuro', icon: 'moon' },
]

const buttons = ref<HTMLButtonElement[]>([])

/** Flechas izquierda/derecha recorren el grupo, como un grupo de radio. */
async function onKey(e: KeyboardEvent, index: number) {
  if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return
  e.preventDefault()
  const step = e.key === 'ArrowRight' ? 1 : -1
  const next = (index + step + OPTIONS.length) % OPTIONS.length
  settings.setTheme(OPTIONS[next].value)
  await nextTick()
  buttons.value[next]?.focus()
}
</script>

<template>
  <div class="theme-switch" role="radiogroup" aria-label="Tema">
    <button
      v-for="(option, i) in OPTIONS"
      :key="option.value"
      ref="buttons"
      class="theme-option"
      role="radio"
      :aria-checked="settings.theme === option.value"
      :aria-label="option.label"
      :title="option.label"
      :tabindex="settings.theme === option.value ? 0 : -1"
      @click="settings.setTheme(option.value)"
      @keydown="onKey($event, i)"
    >
      <AppIcon :name="option.icon" />
    </button>
  </div>
</template>
