---
name: "servidor-local-tauri"
description: "Guía maestra de la app de escritorio tipo Laragon de SOGLOTEC (Tauri 2 + Rust + Vue 3) que gestiona Nginx, PHP y MariaDB con sitios .test. Úsala siempre al trabajar en ese repositorio, aunque la tarea parezca pequeña: comandos Tauri, drivers, supervisor, plantillas, UI o build de Windows."
---

# Servidor local tipo Laragon (Tauri 2 + Vue 3)

App de escritorio interna de SOGLOTEC. Gestiona Nginx, PHP y MariaDB como procesos propios y crea un dominio `.test` por cada carpeta de proyecto. Se usa en el trabajo diario y para enseñar WordPress y Laravel a pasantes, así que la prioridad es que sea simple, predecible y que los fallos se vean con un mensaje claro.

Referencia funcional: Laragon (concepto y simplicidad). Referencia de implementación abierta: FlyEnv (github.com/xpf0000/FlyEnv).

## Reglas del proyecto

1. **Hexagonal.** El núcleo (dominio, casos de uso, supervisor, drivers) nunca llama al sistema operativo directamente: pide lo que necesita a un puerto (trait) y un adaptador por plataforma lo resuelve. Así el port a macOS es solo escribir adaptadores.
2. **La interfaz no tiene lógica de negocio.** Vue solo hace `invoke` y `listen`. El estado de un servicio cambia en pantalla únicamente al recibir un evento, nunca por suponer que el comando funcionó.
3. **Todo bajo una raíz portable.** Sin servicios de Windows, sin tocar el registro, sin modificar el PATH global. La única excepción es el archivo hosts.
4. **Lo generado no se edita.** Los archivos de `etc/` se regeneran desde plantillas; lo personalizado va en archivos de inclusión aparte.
5. **Identidad propia.** No usar el nombre, logo ni íconos de Laragon. El nombre de la app está pendiente: usar el marcador `APP_NAME` de `tauri.conf.json` y no inventar uno.
6. **Windows 10 o superior.** Ver la tabla de plataformas.
7. **Idioma.** Textos de interfaz y mensajes de error en español; identificadores de código en inglés.

## Plataformas

| Plataforma | Estado |
| --- | --- |
| Windows 10 y 11 (x64) | Objetivo de la fase 1; aquí se prueba primero |
| macOS | Fase 3, solo adaptadores nuevos |
| Windows 7, 8 y 8.1 | No soportado |

Por qué no Windows 7: WebView2 quedó congelado en la versión 109 para Windows 7/8/8.1, sin parches; Rust estable exige Windows 10 desde la 1.78 (Windows 7 requiere el target `x86_64-win7-windows-msvc`, tier 3, con nightly y `-Z build-std`); y PHP 8.3 o superior exige Windows 8, mientras Laravel 13 exige PHP 8.3. Si alguien pide soporte para Windows 7, explicar esto antes de tocar código.

## Stack

- **Shell:** Tauri 2. **Núcleo:** Rust estable, `tokio`, `serde`, `thiserror`, `async-trait`.
- **Plantillas:** `minijinja`, embebidas con `include_str!`.
- **Observador de archivos:** `notify`.
- **Plugins Tauri:** `tauri-plugin-opener` (abrir URL y carpetas), `tauri-plugin-single-instance`, `tauri-plugin-dialog`. Bandeja con la feature `tray-icon`.
- **Interfaz:** Vue 3 con `<script setup lang="ts">`, Pinia, Vite. Sin router (una sola ventana). CSS propio con variables; sin framework de UI.
- **Empaquetado:** instalador NSIS. `bundle.windows.webviewInstallMode` en `embedBootstrapper`; usar `offlineInstaller` si las PC no tienen internet.

Perfil de release para un binario pequeño:

```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

## Estructura del repositorio

```text
src/                      interfaz Vue 3
  App.vue
  components/             ServiceRow, ActionBar, TreeMenu
  stores/                 services.ts, sites.ts, settings.ts
  types/ipc.ts            espejo manual de los tipos de Rust
src-tauri/
  tauri.conf.json
  capabilities/default.json
  templates/              nginx.conf, vhost.conf, php.ini, my.cnf
  src/
    main.rs               atiende --apply-hosts y luego llama a lib::run
    lib.rs                builder de Tauri, estado, bandeja, registro de comandos
    error.rs              AppError (thiserror + Serialize)
    domain/               ServiceId, ServiceState, Site, InstalledVersion, AppConfig
    ports/                traits hacia el sistema operativo
    adapters/
      windows/
      macos/
      shared/             archivos, descargas, config en JSON
    drivers/              nginx.rs, php.rs, mariadb.rs
    supervisor/           estado y ciclo de vida de los procesos
    usecases/             start_all, stop_all, switch_php, sync_sites, create_site
    commands/             funciones #[tauri::command], delgadas
    tray.rs
.github/workflows/build-windows.yml
```

El código específico de plataforma vive solo en `adapters/` y, dentro de `drivers/php.rs`, detrás de `#[cfg(windows)]` y `#[cfg(target_os = "macos")]`.

## Carpeta de datos

Raíz por defecto `C:\APP_NAME`, configurable. Evitar rutas con espacios: Nginx y PHP dan problemas con ellas.

```text
<raíz>/
  bin/
    nginx/<versión>/
    php/<versión>/
    mariadb/<versión>/
  www/               una subcarpeta = un sitio
  etc/
    nginx/sites/     un archivo por sitio, generado
    nginx/custom/    inclusiones del usuario, nunca se regeneran
    php/
    mariadb/
    ssl/
  data/mariadb/
  logs/
  tmp/               PID, temporales de Nginx
  app.json           versión activa por servicio, puertos, preferencias
```

En la fase 1 los binarios se copian a mano en `bin/` desde los sitios oficiales (windows.php.net, nginx.org, mariadb.org). Una versión se detecta por carpeta y se valida por la presencia del ejecutable (`php-cgi.exe`, `nginx.exe`, `bin/mariadbd.exe`).

## Puertos (traits)

```rust
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn spawn(&self, spec: &ProcessSpec) -> Result<ProcessHandle, AppError>;
    async fn stop_tree(&self, pid: u32, grace: Duration) -> Result<(), AppError>;
    fn is_alive(&self, pid: u32) -> bool;
}

#[async_trait]
pub trait HostsManager: Send + Sync {
    async fn list(&self) -> Result<Vec<HostEntry>, AppError>;
    /// Reescribe solo el bloque marcado de la app; pide elevación.
    async fn sync(&self, entries: &[HostEntry]) -> Result<(), AppError>;
}

pub trait PortChecker: Send + Sync {
    fn is_free(&self, port: u16) -> bool;
    fn who_uses(&self, port: u16) -> Option<String>;
}

pub trait BinaryProvider: Send + Sync {
    fn list_installed(&self, service: ServiceId) -> Result<Vec<InstalledVersion>, AppError>;
}

pub trait ShellLauncher: Send + Sync {
    fn open_terminal(&self, cwd: &Path, env: &HashMap<String, String>) -> Result<(), AppError>;
}

pub trait ConfigStore: Send + Sync {
    fn load(&self) -> Result<AppConfig, AppError>;
    fn save(&self, cfg: &AppConfig) -> Result<(), AppError>;
}
```

`CertificateAuthority` (SSL) se añade en la fase 2 y la instalación de versiones desde la app (`BinaryProvider::install`) en la fase 4. Los adaptadores se inyectan como `Arc<dyn Trait>` en `AppState`.

## Drivers de servicio

```rust
#[async_trait]
pub trait ServiceDriver: Send + Sync {
    fn id(&self) -> ServiceId;
    fn depends_on(&self) -> &[ServiceId];
    fn default_port(&self) -> u16;
    async fn render_config(&self, ctx: &RenderContext) -> Result<(), AppError>;
    async fn first_run_setup(&self, _ctx: &RenderContext) -> Result<(), AppError> { Ok(()) }
    fn start_specs(&self, ctx: &RenderContext) -> Vec<ProcessSpec>;
    async fn stop(&self, ctx: &RenderContext, pids: &[u32], runner: &dyn ProcessRunner) -> Result<(), AppError>;
    async fn health(&self, ctx: &RenderContext) -> Health;
    async fn reload(&self, _ctx: &RenderContext) -> Result<(), AppError> { Ok(()) }
}
```

| Servicio | Arranque | Parada | Salud |
| --- | --- | --- | --- |
| Nginx | `nginx -p <raíz> -c etc/nginx/nginx.conf` | `nginx -s quit` con los mismos `-p` y `-c` | Conexión TCP al puerto 80 |
| PHP en Windows | N procesos `php-cgi.exe -b 127.0.0.1:900N -c etc/php/php.ini` | Terminar los procesos | Conexión TCP a cada puerto |
| PHP en macOS | `php-fpm -F -y php-fpm.conf` | Señal de terminación | Conexión TCP |
| MariaDB | `mariadbd --defaults-file=etc/mariadb/my.cnf` | `mariadb-admin -u root shutdown` | `mariadb-admin ping` |

Detalles que evitan errores conocidos:

- **Nginx:** en la configuración usar barras `/` aunque sea Windows. Declarar las rutas temporales y el PID dentro de `tmp/`. Al cambiar sitios usar `nginx -s reload`, no reiniciar.
- **PHP en Windows:** usar builds NTS x64. Cada `php-cgi` atiende una petición a la vez, por eso se lanzan varios (4 por defecto) y Nginx reparte con un `upstream php_upstream`. Lanzarlos con `PHP_FCGI_MAX_REQUESTS=0`; sin eso cada proceso termina solo tras 500 peticiones. `PHP_FCGI_CHILDREN` no funciona en Windows.
- **php.ini:** generarlo con `extension_dir = "ext"` y las extensiones que piden Laravel y WordPress: `curl`, `fileinfo`, `gd`, `intl`, `mbstring`, `openssl`, `pdo_mysql`, `mysqli`, `zip`, `exif`.
- **Runtime de Visual C++:** PHP lo necesita. Si falta, mostrar un error que lo diga, no un fallo genérico.
- **MariaDB:** la primera vez correr `mariadb-install-db --datadir=data/mariadb`. `bind-address = 127.0.0.1` y `utf8mb4`. Root sin contraseña porque es solo local. Nunca matar el proceso a la fuerza salvo que el apagado ordenado no responda: se dañan tablas.
- **Cambio de versión de PHP:** guardar la versión en `app.json`, regenerar su configuración y reiniciar solo PHP. Nginx no cambia porque el `upstream` apunta siempre a los mismos puertos.

## Supervisor

Único componente que conoce el estado de los servicios y el único que lanza o detiene procesos. Estados: `stopped`, `starting`, `running`, `stopping`, `failed`. Cada cambio se emite como evento `service:state`.

Secuencia de "Iniciar todo":

1. Regenerar la configuración de los tres servicios.
2. Comprobar puertos. Si uno está ocupado, el servicio pasa a `failed` con el nombre del programa que lo ocupa (típico en PC de pasantes: XAMPP, IIS, otro MySQL).
3. Lanzar MariaDB y PHP en paralelo; Nginx al final.
4. Sondear la salud hasta que responda o venzan 15 segundos.
5. Pasar a `running`, o a `failed` con las últimas líneas del log.

Reglas:

- Parada ordenada con 10 segundos de plazo; solo después se fuerza.
- Si un proceso termina solo, pasa a `failed` y no se reinicia automáticamente. Para enseñar conviene que el fallo se vea.
- Al salir de la app se detienen todos los servicios. Cerrar la ventana no es salir: la oculta en la bandeja.
- Guardar los PID en `tmp/` y limpiar huérfanos al abrir.

## Sitios y archivo hosts

Cada subcarpeta de `www/` es un sitio `<carpeta>.test`. Un observador vigila el primer nivel de `www/` y dispara `sync_sites`, que genera o borra `etc/nginx/sites/<carpeta>.conf`, sincroniza el bloque del archivo hosts y recarga Nginx.

Raíz pública: si existe `public/` se usa esa carpeta (Laravel); si no, la del sitio (WordPress).

```nginx
server {
    listen 80;
    server_name {{ domain }};
    root "{{ root }}";
    index index.php index.html;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        include fastcgi_params;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
        fastcgi_pass php_upstream;
    }

    include "{{ custom_dir }}/{{ name }}.conf*";
}
```

El archivo hosts solo se toca dentro de un bloque entre dos comentarios marcadores; el resto no se modifica nunca. Para la elevación, la app se relanza a sí misma con UAC en modo auxiliar: `main.rs` detecta `--apply-hosts <archivo>`, escribe el bloque y termina, sin abrir ventana. El adaptador lo invoca con `Start-Process -Verb RunAs -Wait`. Agrupar los cambios para pedir permiso una sola vez.

## Contrato entre núcleo e interfaz

| Comando (`invoke`) | Devuelve |
| --- | --- |
| `services_start_all`, `services_stop_all` | nada |
| `services_start`, `services_stop` (`id`) | nada |
| `services_snapshot` | estado actual de todos, para pintar al abrir |
| `php_list_versions`, `php_switch_version` (`version`) | lista / nada |
| `sites_list` | `Site[]` |
| `sites_create` (`name`, `preset`: blank, wordpress, laravel) | `Site` (fase 2) |
| `shell_open_terminal` (`cwd` opcional), `shell_open_root`, `shell_open_web` | nada |

| Evento (`listen`) | Carga |
| --- | --- |
| `service:state` | `{ id, state, detail? }` |
| `sites:changed` | `Site[]` |

Los comandos son delgados: llaman a un caso de uso y devuelven `Result<T, AppError>`. Los tipos usan `#[serde(rename_all = "camelCase")]` y se reflejan a mano en `src/types/ipc.ts`; al cambiar uno, cambiar el otro en el mismo commit.

```rust
#[tauri::command]
pub async fn services_start_all(state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    state.supervisor.start_all().await
}
```

```ts
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export const useServicesStore = defineStore('services', () => {
  const byId = ref<Record<ServiceId, ServiceStatus>>({} as never)

  async function init() {
    for (const s of await invoke<ServiceStatus[]>('services_snapshot')) byId.value[s.id] = s
    await listen<ServiceStatus>('service:state', (e) => { byId.value[e.payload.id] = e.payload })
  }

  const startAll = () => invoke('services_start_all')
  return { byId, init, startAll }
})
```

## Interfaz

Una sola ventana compacta de tamaño fijo, sin navegación.

| Zona | Contenido |
| --- | --- |
| Cabecera | Nombre de la app, versión y botón de menú |
| Lista de servicios | Una fila por servicio: punto de estado, nombre, versión activa, puerto, recargar |
| Barra de acciones | Iniciar todo / Detener, Web, Base de datos, Terminal, Raíz |
| Menú en árbol | Sitios, PHP (versión, php.ini, extensiones), Nginx, MariaDB, Logs, Preferencias |
| Bandeja | El mismo menú; se reconstruye al recibir `sites:changed` |

Un servicio en `failed` muestra su `detail` en la misma fila, sin diálogos modales. La terminal se abre con un PATH propio (PHP activo, `mariadb/bin`, Nginx, Composer) que no se escribe en el sistema.

## Detalles de Windows

- Lanzar todos los procesos hijos con `CREATE_NO_WINDOW` (`0x08000000`); si no, aparece una consola por cada uno.
- Asignar los hijos a un Job Object con cierre en cascada (por ejemplo con el crate `win32job`) para que mueran si la app se cierra de forma inesperada.
- Para detener un árbol de procesos sin apagado propio: `taskkill /PID <pid> /T`, y `/F` solo tras vencer el plazo.
- `who_uses` se resuelve con `netstat -ano` más `tasklist`.
- El instalador va sin firma: SmartScreen avisará al ejecutarlo. Es aceptado por ser uso interno.

## Cómo trabajar en este repositorio

Las sesiones en la nube corren en Linux (Ubuntu). Ahí no se puede abrir la ventana ni ejecutar el código de Windows, así que:

- Mantener dominio, drivers (render de plantillas, armado de `ProcessSpec`) y casos de uso libres de llamadas al sistema, y cubrirlos con pruebas unitarias que corren en cualquier plataforma usando adaptadores falsos.
- Antes de cada commit: `npm run build` (incluye `vue-tsc`), `cargo fmt --check`, `cargo clippy` y `cargo test` dentro de `src-tauri/`.
- Si `cargo` falla en Linux por bibliotecas de sistema, instalar: `apt install -y libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev build-essential`.
- La validación real de Windows es el job de GitHub Actions. No afirmar que algo funciona en Windows sin que ese job pase; decir qué se probó y qué no.
- El instalador para probar en Windows 10 se descarga del artefacto `instalador-windows` de la ejecución.

`.github/workflows/build-windows.yml`:

```yaml
name: build-windows
on:
  push:
    branches: [main]
  pull_request:
  workflow_dispatch:
jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: npm
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: src-tauri
      - run: npm ci
      - run: cargo test --manifest-path src-tauri/Cargo.toml
      - run: npm run tauri build
      - uses: actions/upload-artifact@v4
        with:
          name: instalador-windows
          path: src-tauri/target/release/bundle/nsis/*.exe
```

Requiere el script `"tauri": "tauri"` en `package.json`. Si el push que incluye `.github/workflows/` es rechazado por permisos, pedir al usuario que cree ese archivo desde GitHub.

## Fases

| Fase | Plataforma | Alcance |
| --- | --- | --- |
| 1. Núcleo | Windows 10 | Iniciar y detener todo o un servicio; cambio de versión de PHP; sitio `.test` por carpeta; bandeja; botones Web, Base de datos, Terminal y Raíz |
| 2. Enseñanza | Windows | Crear sitio WordPress o Laravel con un clic; SSL automático; acceso a configuraciones y logs |
| 3. macOS | macOS | Adaptadores de macOS; `php-fpm`; binarios desde Homebrew |
| 4. Extras | Ambas | Descarga de versiones desde la app; respaldos programados; Mailpit; Redis |

Fuera de alcance: Apache, PostgreSQL, compartir sitios por túnel, gestión de Node o Python, actualización automática. No adelantar trabajo de una fase posterior sin que se pida.

## Pendiente de decidir

Preguntar al usuario en lugar de asumir:

- Nombre e identidad visual de la app.
- Versiones de PHP y MariaDB que se incluyen en la fase 1.
- Cliente de base de datos que abre el botón "Base de datos".
- Si los pasantes con Mac pueden instalar Homebrew.