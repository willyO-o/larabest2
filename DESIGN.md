---
name: LARABEST
description: Servidor local con la estructura de Laragon; la ventana es la luz de estado de la pila, con el azul eléctrico y el azul marino del logo de LaraBest.
colors:
  ground: "#f3f6fb"
  surface: "#ffffff"
  sunken: "#eaeff7"
  line: "#d9e0ec"
  ink: "#141c30"
  ink-2: "#47536d"
  ink-3: "#5f6b85"
  accent: "#2563eb"
  accent-hover: "#1d4ed8"
  ok: "#15803d"
  busy: "#b45309"
  bad: "#b42318"
  bad-ground: "#fdeceb"
  warn: "#9a5b06"
  band-stopped: "#1b2440"
  band-busy: "#7a4a0a"
  band-partial: "#1e4a7a"
  band-running: "#14633f"
  band-failed: "#8a1c1c"
  dark-ground: "#0d1322"
  dark-surface: "#141c2f"
  dark-ink: "#e7ecf6"
  dark-accent: "#6ea6ff"
  dark-accent-hover: "#8bb8ff"
  dark-accent-ink: "#08142b"
  dark-sunken: "#1a2339"
  dark-line: "#26314b"
  dark-ink-2: "#b1bcd2"
  dark-ink-3: "#8d99b3"
  dark-ok: "#4ade80"
  dark-busy: "#f5b14c"
  dark-bad: "#ff8a80"
  dark-bad-ground: "#3a1d1c"
  led-busy: "#fbbf24"
  wordmark-accent: "#b4d2ff"
  dark-band-stopped: "#182340"
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
  title:
    fontFamily: "'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 700
    lineHeight: 1.35
    letterSpacing: "normal"
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
  pill: "999px"
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

## Marca

El logo (`src-tauri/icons/logo.svg`, del que salen todos los íconos con `npx tauri icon`) es un servidor isométrico de tres bandejas en azul marino con `</>` en azul eléctrico, dentro de un anillo azul sobre disco blanco. En la banda va a 26 px con una sombra mínima, seguido de la palabra en dos tonos: «Lara» en blanco y «Best» en `#b4d2ff`, el azul claro que pasa 4,5:1 sobre los seis colores de la banda. El nombre técnico de la app (ventana, instalador) sigue siendo LARABEST.

- **Azul eléctrico** (`accent`): la acción principal, el foco, la selección y los enlaces. En oscuro se aclara (`#6ea6ff`) con texto oscuro encima.
- **Azul marino** (`band-stopped`, `ink`): la banda en reposo y el texto. Los neutros llevan un leve tinte azul frío; el oscuro es azul marino profundo, no gris.
- Los colores de estado (verde, ámbar, rojo) no son marca: son información y no cambian.
- Profundidad con desplazamiento y desenfoque (`--shadow`, azul marino translúcido). El botón principal suma una luz arriba y una sombra azul debajo.

## Overview

LARABEST es una herramienta de trabajo que queda abierta junto al editor y el navegador. Toma de Laragon la estructura que los pasantes ya conocen: una ventana compacta, una fila de texto por servicio con sus botones Recargar y Detener, cinco botones grandes abajo y un menú contextual nativo en árbol con clic derecho, el botón Menú y la bandeja. La identidad es propia: nombre, marca de tres bandejas apiladas, índigo como acento y una banda superior que cambia de color con el estado de toda la pila, para leerla de reojo aunque la ventana esté medio tapada.

Modo Operate: lo familiar es una virtud. Una sola familia tipográfica del sistema, cifras tabulares, filas densas sin tarjetas y color que siempre hace un trabajo (acción principal o estado).

## Colors

### Primary
- **Índigo de acción** (`accent`, #3d4ed8; oscuro #8b96ff): solo para la acción principal (Iniciar todo) y el foco.

### Neutral
- **Fondo frío** (`ground`, #f3f4f7) y **superficie** (`surface`, #ffffff) para la lista; `sunken` para la línea de estado y los estados hover; `line` para separadores de 1px.
- Tinta en tres pasos (`ink`, `ink-2`, `ink-3`) para nombre, datos y estado secundario.
- Tema oscuro: los mismos roles con prefijo `dark-`; el LED de la marca usa `dark-ok`, `led-busy` y `dark-bad` sobre la banda en ambos temas porque se leen mejor sobre fondos oscuros.

### Named Rules
- **Dos temas, una banda.** Los neutros y los colores de estado tienen versión clara y oscura; la banda conserva sus cinco colores en ambos temas porque comunica estado, no superficie.
- **La banda es el semáforo.** La banda superior usa un color por estado de la pila: grafito detenido, ámbar iniciando o deteniendo, azul petróleo con parte en marcha, verde todo en marcha y rojo con un fallo. El texto siempre es blanco. No se usa para nada más.
- **El estado nunca va solo en color.** Cada punto va acompañado de su palabra (en marcha, detenido, error) y la banda de su resumen ("Todo en marcha", "PHP con error").
- **El rojo explica.** El fondo `bad-ground` solo aparece con el detalle de un fallo o el error de una acción, y siempre lleva la causa y cómo resolverla.

## Typography

Segoe UI Variable (la del sistema en Windows 10/11) en todos los textos; cifras tabulares globales para versiones y puertos.

### Hierarchy
- Título de hoja (Nuevo sitio): 15px, 700.
- Marca: 14px, 700, espaciado 0.06em.
- Nombre de servicio: 14px, 600.
- Texto base, datos y botones: 13px, 400 o 500.
- Línea de estado y versión de la marca: 12px.

## Layout

Ventana fija de 700×380 (crece en alto con los servicios opcionales y los paneles). De arriba abajo: banda (46px), lista de servicios (flexible; rejilla de 6 columnas: punto, nombre, versión —en PHP con su compilación, NTS · x64 · VS17—, puerto, estado, acciones), error de acción si existe, fila de cinco botones (el primero 1.25 veces más ancho) y línea de estado (26px). Márgenes laterales de 12 a 16px, separación de 6 a 8px entre controles.

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
Marca, nombre, versión, resumen del estado alineado a la derecha, selector de tema y botón Menú translúcido. El LED de la marca repite el color del estado.

### Hoja "Nuevo sitio"
Se abre bajo la banda (la banda sigue visible y viva) como `<dialog>` nativo: Esc cierra salvo mientras se crea el sitio. Formulario: campo con vista previa de la URL (`www/<nombre> → https://<nombre>.test`) que pasa a rojo con el motivo si el nombre no sirve; control segmentado de tres plantillas con una línea que explica la elegida; Cancelar y Crear sitio abajo a la derecha. Durante la creación: lista de pasos con marca (punto ámbar que late = en curso, check verde = hecho, cruz roja = falló), aviso ámbar si terminó con advertencias y alerta roja con la causa si falló.

### Control segmentado
Pista `sunken` con 2px de relleno; la opción elegida es superficie con borde `line` de 1px, sin sombra. Flechas izquierda/derecha cambian de opción.

### Selector de tema
Grupo de radio de tres botones de 26px con ícono (monitor, sol, luna) sobre el mismo material translúcido que Menú (`rgb(255 255 255 / 0.12)`); la opción activa sube a 0.26 con ícono blanco pleno. Las flechas izquierda/derecha cambian de opción. "Sistema" sigue a Windows; Claro y Oscuro fijan el tema de la página y de la barra de título. También está en Menú → Preferencias → Tema.

## Do's and Don'ts

### Do:
- Mantener la estructura de Laragon: filas de servicio, cinco botones abajo y todo lo demás en el menú contextual.
- Mostrar cada fallo en su fila, con la causa y la solución.
- Usar color solo para la acción principal y los estados.

### Don't:
- No usar el nombre, el logo ni los íconos de Laragon.
- No convertir las filas en tarjetas ni añadir sombras.
- No crear diálogos para lo que cabe en una fila o en el menú; solo los menús de bandeja muestran errores en un diálogo, porque la ventana puede estar oculta.
