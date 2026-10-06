# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Desarrolladores de SOGLOTEC en su trabajo diario y pasantes que aprenden WordPress y Laravel, en PC con Windows 10 u 11. La app queda abierta junto al editor y el navegador; la miran de reojo para saber si el servidor está en marcha y la usan para arrancar servicios, abrir sitios y ajustar configuraciones.

## Product Purpose

Servidor local portable tipo Laragon: gestiona Nginx, PHP y MariaDB como procesos propios y crea un dominio `<carpeta>.test` por cada carpeta de `www/`. Éxito: un pasante arranca todo con un clic, ve un fallo explicado en la misma fila (puerto ocupado, falta el runtime de Visual C++) y llega a cualquier configuración desde el menú contextual sin buscar archivos.

## Positioning

Uso interno. Mismo modelo mental y estructura que Laragon (ventana compacta, filas de servicios, botones Iniciar todo / Web / Base de datos / Terminal / Raíz, menú contextual en árbol con clic derecho y en la bandeja), con identidad propia y fallos visibles en lugar de reinicios silenciosos, pensado para enseñar.

## Operating Context

Ventana pequeña de tamaño fijo, a menudo parcialmente tapada; bandeja del sistema; navegador para los sitios `.test`; Adminer en `adminer.test`; terminal `cmd` con PATH propio; archivo hosts con elevación UAC.

## Capabilities and Constraints

- Tauri 2 + Rust + Vue 3; la interfaz no tiene lógica de negocio (solo `invoke` y `listen`; el estado cambia solo al recibir eventos).
- Todo bajo una raíz portable (`C:\LARABEST` por defecto), sin servicios de Windows ni registro; solo se toca el archivo hosts.
- Lo generado en `etc/` no se edita: las configuraciones personales van en archivos de inclusión aparte.
- Textos de interfaz en español; identificadores en inglés.
- Fase 1 + configuraciones en el menú contextual (php.ini personal, extensiones, configs de Nginx/MariaDB, logs, preferencias).

## Brand Commitments

- Nombre: **LARABEST**.
- No usar el nombre, logo ni íconos de Laragon; Laragon es referencia de estructura y simplicidad, no de identidad visual.

## Evidence on Hand

No hay logo definitivo ni capturas de usuarios; el ícono actual (`src-tauri/icons/`) es provisional y propio.

## Product Principles

1. El estado se ve de un vistazo y nunca se supone: lo dicta el núcleo.
2. Los fallos se explican donde ocurren, con la causa y cómo resolverla.
3. Cada configuración está a dos clics desde el menú contextual.
4. Simple y predecible antes que completo.

## Accessibility & Inclusion

Contraste AA, estados que no dependen solo del color (texto del estado junto al punto), navegación por teclado y respeto a `prefers-reduced-motion`.
