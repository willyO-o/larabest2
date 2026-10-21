<script setup lang="ts">
// Logo de LaraBest (mismo dibujo que src-tauri/icons/logo.svg): servidor de
// tres bandejas con </> dentro del anillo azul.
</script>

<template>
  <svg class="mark" viewBox="0 0 256 256" aria-hidden="true">
    <circle cx="128" cy="128" r="114" fill="#ffffff" stroke="#1d63ed" stroke-width="13" />
    <polygon points="66,144 128,176 128,200 66,168" fill="#1f2a44" />
    <polygon points="128,176 190,144 190,168 128,200" fill="#283554" />
    <polygon points="66,114 128,146 128,170 66,138" fill="#1f2a44" />
    <polygon points="128,146 190,114 190,138 128,170" fill="#283554" />
    <polygon points="128,52 190,84 128,116 66,84" fill="#3a4766" />
    <polygon points="66,84 128,116 128,140 66,108" fill="#1f2a44" />
    <polygon points="128,116 190,84 190,108 128,140" fill="#283554" />
    <g fill="#ffffff">
      <circle cx="160" cy="111.5" r="5" />
      <circle cx="174" cy="104.3" r="5" />
      <circle cx="160" cy="141.5" r="5" />
      <circle cx="174" cy="134.3" r="5" />
      <circle cx="160" cy="171.5" r="5" />
      <circle cx="174" cy="164.3" r="5" />
    </g>
    <g fill="none" stroke-linecap="round" stroke-linejoin="round">
      <path
        d="M160 168 L144 182 L160 196 M184 160 L170 204 M194 168 L210 182 L194 196"
        stroke="#ffffff"
        stroke-width="26"
      />
      <path
        d="M160 168 L144 182 L160 196 M184 160 L170 204 M194 168 L210 182 L194 196"
        stroke="#1d63ed"
        stroke-width="13"
      />
    </g>
  </svg>
</template>
