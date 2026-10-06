---
name: LARABEST
description: Servidor local para Windows con la estructura de Laragon; la ventana es la luz de estado de la pila.
colors:
  ground: "#f3f4f7"
  surface: "#ffffff"
  sunken: "#eceef2"
  line: "#dde0e7"
  ink: "#15171c"
  ink-2: "#4f5562"
  ink-3: "#6b7280"
  accent: "#3d4ed8"
  accent-hover: "#3443c4"
  ok: "#15803d"
  busy: "#b45309"
  bad: "#b42318"
  bad-ground: "#fdeceb"
  warn: "#9a5b06"
  band-stopped: "#2b2f38"
  band-busy: "#7a4a0a"
  band-partial: "#23495a"
  band-running: "#155e37"
  band-failed: "#8a1c1c"
  dark-ground: "#16181d"
  dark-surface: "#1e2128"
  dark-ink: "#e9ebf0"
  dark-accent: "#8b96ff"
typography:
  body:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.35
    letterSpacing: "normal"
  service-name:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "normal"
  wordmark:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 700
    lineHeight: 1.35
    letterSpacing: "0.06em"
  status:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.35
    letterSpacing: "normal"
rounded:
  sm: "4px"
  md: "6px"
  lg: "8px"
spacing:
  xs: "4px"
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
components:
  band:
    backgroundColor: "{colors.band-stopped}"
    textColor: "#ffffff"
    height: "46px"
    padding: "0 8px 0 14px"
  button-main:
    backgroundColor: "{colors.accent}"
    textColor: "#ffffff"
    rounded: "{rounded.lg}"
    height: "64px"
  button-main-hover:
    backgroundColor: "{colors.accent-hover}"
  button-action:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    height: "64px"
  button-row:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    height: "28px"
    padding: "0 10px"
  failure-detail:
    backgroundColor: "{colors.bad-ground}"
    textColor: "{colors.bad}"
    rounded: "{rounded.md}"
    padding: "7px 10px"
  statusline:
    backgroundColor: "{colors.sunken}"
    textColor: "{colors.ink-3}"
    typography: "{typography.status}"
    height: "26px"
---

# Design System: LARABEST

## Overview

LARABEST es una herramienta de trabajo que queda abierta junto al editor y el navegador. Toma de Laragon la estructura que los pasantes ya conocen: una ventana compacta, una fila de texto por servicio con sus botones Recargar y Detener, cinco botones grandes abajo y un menú contextual nativo en árbol con clic derecho, el botón Menú y la bandeja. La identidad es propia: nombre, marca de tres bandejas apiladas, índigo como acento y una banda superior que cambia de color con el estado de toda la pila, para leerla de reojo aunque la ventana esté medio tapada.

Modo Operate: lo familiar es una virtud. Una sola familia tipográfica del sistema, cifras tabulares, filas densas sin tarjetas y color que siempre hace un trabajo (acción principal o estado).

## Colors

### Primary
- **Índigo de acción** (`accent`, #3d4ed8; oscuro #8b96ff): solo para la acción principal (Iniciar todo) y el foco.

### Neutral
- **Fondo frío** (`ground`, #f3f4f7) y **superficie** (`surface`, #ffffff) para la lista; `sunken` para la línea de estado y los estados hover; `line` para separadores de 1px.
- Tinta en tres pasos (`ink`, `ink-2`, `ink-3`) para nombre, datos y estado secundario.

### Named Rules
- **La banda es el semáforo.** La banda superior usa un color por estado de la pila: grafito detenido, ámbar iniciando o deteniendo, azul petróleo con parte en marcha, verde todo en marcha y rojo con un fallo. El texto siempre es blanco. No se usa para nada más.
- **El estado nunca va solo en color.** Cada punto va acompañado de su palabra (en marcha, detenido, error) y la banda de su resumen ("Todo en marcha", "PHP con error").
- **El rojo explica.** El fondo `bad-ground` solo aparece con el detalle de un fallo o el error de una acción, y siempre lleva la causa y cómo resolverla.

## Typography

Segoe UI Variable (la del sistema en Windows 10/11) en todos los textos; cifras tabulares globales para versiones y puertos.

### Hierarchy
- Marca: 14px, 700, espaciado 0.06em.
- Nombre de servicio: 14px, 600.
- Texto base, datos y botones: 13px, 400 o 500.
- Línea de estado y versión de la marca: 12px.

## Layout

Ventana fija de 640×380. De arriba abajo: banda (46px), lista de servicios (flexible; rejilla de 6 columnas: punto, nombre, versión, puerto, estado, acciones), error de acción si existe, fila de cinco botones (el primero 1.25 veces más ancho) y línea de estado (26px). Márgenes laterales de 12 a 16px, separación de 6 a 8px entre controles.

## Elevation & Depth

Plano: sin sombras. La profundidad la dan el fondo (`ground` frente a `surface`) y los separadores de 1px.

## Shapes

Radios pequeños y constantes: 6px en botones de fila y detalles, 8px en los botones grandes y el menú, 50% en los puntos de estado.

## Components

### Buttons
- **Principal** (Iniciar todo): índigo con texto blanco; con servicios en marcha pasa a "Detener" sobre superficie con texto rojo.
- **Acción** (Web, Base de datos, Terminal, Raíz): superficie con borde, ícono de 20px encima de la etiqueta.
- **De fila** (Recargar, Iniciar/Detener): 28px de alto y 74px de ancho mínimo; cuando no aplican quedan como texto gris sin borde.
- Todos se encogen a 0.97 al pulsar (140ms con ease-out fuerte); sin animación con `prefers-reduced-motion`.

### Navigation
- **Menú contextual nativo** en árbol: el mismo menú en clic derecho, botón Menú y bandeja (www, Nginx, PHP, MariaDB, Logs, Herramientas, Preferencias). Usa los controles del sistema: marcas de verificación para la versión activa, las extensiones y las preferencias.

### Banda de estado (componente propio)
Marca, nombre, versión, resumen del estado alineado a la derecha y botón Menú translúcido. El LED de la marca repite el color del estado.

## Do's and Don'ts

### Do:
- Mantener la estructura de Laragon: filas de servicio, cinco botones abajo y todo lo demás en el menú contextual.
- Mostrar cada fallo en su fila, con la causa y la solución.
- Usar color solo para la acción principal y los estados.

### Don't:
- No usar el nombre, el logo ni los íconos de Laragon.
- No convertir las filas en tarjetas ni añadir sombras.
- No crear diálogos para lo que cabe en una fila o en el menú; solo los menús de bandeja muestran errores en un diálogo, porque la ventana puede estar oculta.
