<script setup lang="ts">
import type { ServiceId } from '../types/ipc'

// Distintivo de cada servicio con su color de marca, dibujado aquí (sin
// archivos externos). Es decorativo: el nombre va al lado.
defineProps<{ id: ServiceId }>()
</script>

<template>
  <svg class="service-logo" viewBox="0 0 32 28" aria-hidden="true">
    <!-- Nginx: hexágono verde con la N. -->
    <template v-if="id === 'nginx'">
      <path d="M16 1.5l12 7v11l-12 7-12-7v-11z" fill="#009639" />
      <path
        d="M11.5 19.5v-11l9 11v-11"
        fill="none"
        stroke="#fff"
        stroke-width="2.6"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </template>

    <!-- Apache: pluma en degradado rojo-naranja. -->
    <template v-else-if="id === 'apache'">
      <defs>
        <linearGradient id="logo-apache" x1="0" y1="1" x2="1" y2="0">
          <stop offset="0" stop-color="#bd1f2d" />
          <stop offset="0.55" stop-color="#e7522a" />
          <stop offset="1" stop-color="#f6a723" />
        </linearGradient>
      </defs>
      <path
        d="M8 25c3-7 8.5-15.5 17-22.5-1.5 6-4 11.5-8 15.5-2.5 2.5-5.5 4-9 7z"
        fill="url(#logo-apache)"
      />
      <path d="M8 25c3-5 6-9 10-13" fill="none" stroke="#fff" stroke-opacity="0.7" stroke-width="1.2" stroke-linecap="round" />
    </template>

    <!-- PHP: óvalo violeta con el nombre. -->
    <template v-else-if="id === 'php'">
      <ellipse cx="16" cy="14" rx="15.5" ry="9.5" fill="#777bb3" />
      <text x="16" y="17.6" text-anchor="middle" class="logo-text logo-php">php</text>
    </template>

    <!-- MariaDB: el azul petróleo y el bronce de su marca. -->
    <template v-else-if="id === 'mariadb'">
      <rect x="3" y="1" width="26" height="26" rx="7" fill="#003545" />
      <path
        d="M8.5 19.5c1.5-6 4.5-10 9-11.5 2.5-.8 4.6.3 5.5 2.2-2.2-.2-3.8.6-4.7 2.3-1 2-1.6 4.4-3.8 6.2-1.6 1.2-3.7 1.4-6 .8z"
        fill="#c49a6c"
      />
    </template>

    <!-- PostgreSQL. -->
    <template v-else-if="id === 'postgresql'">
      <rect x="3" y="1" width="26" height="26" rx="7" fill="#336791" />
      <text x="16" y="18.4" text-anchor="middle" class="logo-text">Pg</text>
    </template>

    <!-- Redis: tres capas, como su cubo. -->
    <template v-else-if="id === 'redis'">
      <rect x="3" y="1" width="26" height="26" rx="7" fill="#dc382d" />
      <g fill="#fff">
        <path d="M16 6.5l7.5 3-7.5 3-7.5-3z" />
        <path d="M8.5 13.2l7.5 3 7.5-3v2l-7.5 3-7.5-3z" opacity="0.85" />
        <path d="M8.5 17.8l7.5 3 7.5-3v2l-7.5 3-7.5-3z" opacity="0.7" />
      </g>
    </template>

    <!-- MongoDB: la hoja verde. -->
    <template v-else-if="id === 'mongodb'">
      <rect x="3" y="1" width="26" height="26" rx="7" fill="#13aa52" />
      <path d="M16 4.5c3.5 4 5 7.5 5 10.5 0 3.5-2.2 5.8-5 6.8-2.8-1-5-3.3-5-6.8 0-3 1.5-6.5 5-10.5z" fill="#fff" />
      <path d="M16 9v16" stroke="#13aa52" stroke-width="1.4" stroke-linecap="round" />
    </template>

    <!-- Mailpit: sobre. -->
    <template v-else>
      <rect x="3" y="1" width="26" height="26" rx="7" fill="#0b7285" />
      <path
        d="M9 9h14v10H9zM9 9.5l7 5.5 7-5.5"
        fill="none"
        stroke="#fff"
        stroke-width="1.8"
        stroke-linejoin="round"
        stroke-linecap="round"
      />
    </template>
  </svg>
</template>
