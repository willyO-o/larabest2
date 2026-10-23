<script setup lang="ts">
// Íconos de trazo propios (24x24), sin dependencias.
const PATHS = {
  play: 'M7 5.5v13l11-6.5z',
  stop: 'M7 7h10v10H7z',
  refresh: 'M20 11a8 8 0 1 0-2.3 5.7M20 5v6h-6',
  globe: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3.5 9h17M3.5 15h17M12 3c2.5 2.5 3.5 5.5 3.5 9s-1 6.5-3.5 9M12 3C9.5 5.5 8.5 8.5 8.5 12s1 6.5 3.5 9',
  database: 'M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3',
  terminal: 'M4 5h16v14H4zM8 10l2.5 2L8 14M13 14h3',
  folder: 'M3 6.5V18a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1V8.5a1 1 0 0 0-1-1h-8L10 5H4a1 1 0 0 0-1 1.5z',
  menu: 'M4 7h16M4 12h16M4 17h16',
  chevron: 'M9 6l6 6-6 6',
  close: 'M6 6l12 12M18 6L6 18',
  alert: 'M12 8v5M12 16.5v.5M10.3 4.2L2.8 17.5A2 2 0 0 0 4.5 20.5h15a2 2 0 0 0 1.7-3L13.7 4.2a2 2 0 0 0-3.4 0z',
  plus: 'M12 5v14M5 12h14',
  check: 'M5 12.5l4.5 4.5L19 7',
  lock: 'M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z',
  monitor: 'M4 5h16v11H4zM9 20h6M12 16v4',
  sun: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2.5v2M12 19.5v2M4.6 4.6l1.4 1.4M18 18l1.4 1.4M2.5 12h2M19.5 12h2M4.6 19.4L6 18M18 6l1.4-1.4',
  moon: 'M20 14.6A8 8 0 0 1 9.4 4a8 8 0 1 0 10.6 10.6z',
  hosts: 'M14 3H6a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V8zM14 3v5h5M8.5 12.5h7M8.5 16.5h4.5',
  external: 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5',
  gear: 'M10.4 5.2L10.7 2.6L13.3 2.6L13.6 5.2A7 7 0 0 1 15.7 6.1L17.7 4.4L19.6 6.3L17.9 8.3A7 7 0 0 1 18.8 10.4L21.4 10.7L21.4 13.3L18.8 13.6A7 7 0 0 1 17.9 15.7L19.6 17.7L17.7 19.6L15.7 17.9A7 7 0 0 1 13.6 18.8L13.3 21.4L10.7 21.4L10.4 18.8A7 7 0 0 1 8.3 17.9L6.3 19.6L4.4 17.7L6.1 15.7A7 7 0 0 1 5.2 13.6L2.6 13.3L2.6 10.7L5.2 10.4A7 7 0 0 1 6.1 8.3L4.4 6.3L6.3 4.4L8.3 6.1A7 7 0 0 1 10.4 5.2zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
} as const

export type IconName = keyof typeof PATHS

defineProps<{ name: IconName }>()
</script>

<template>
  <svg
    class="icon"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="PATHS[name]" />
  </svg>
</template>
