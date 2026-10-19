// Espejo manual de los tipos de Rust (src-tauri/src/domain, error.rs y
// commands). Al cambiar uno, cambiar el otro en el mismo commit.

export type ServiceId =
  | 'nginx'
  | 'apache'
  | 'php'
  | 'mariadb'
  | 'mysql'
  | 'postgresql'
  | 'redis'
  | 'mongodb'
  | 'mailpit'

export type ServiceState = 'stopped' | 'starting' | 'running' | 'stopping' | 'failed'

/** Carga de `service:state` y elementos de `services_snapshot`. */
export interface ServiceStatus {
  id: ServiceId
  state: ServiceState
  detail?: string
  version: string | null
  /** Puerto principal. */
  port: number
  /** Todos los puertos que usa (PHP en Windows usa varios; Mailpit, SMTP y web). */
  ports: number[]
  /** Forma parte de la pila: se muestra y «Iniciar todo» lo arranca. */
  enabled: boolean
}

/** Carga de `sites:changed` y resultado de `sites_list`. */
export interface Site {
  name: string
  domain: string
  url: string
  path: string
  documentRoot: string
  /** `false` si el dominio aún no está en el archivo hosts. */
  inHosts: boolean
}

export type SitePreset = 'blank' | 'wordpress' | 'laravel' | 'git' | 'template'

/** Plantilla propia de `etc/sitios.conf` (`sites_templates`). */
export interface SiteTemplate {
  name: string
  /** Comando de Composer, URL de un .zip u otro comando, con %s como nombre. */
  action: string
}

export type ProgressState = 'running' | 'done' | 'failed'

/** Carga de `sites:progress` mientras se crea un sitio. */
export interface SiteProgress {
  name: string
  preset: SitePreset
  step: string
  state: ProgressState
  detail?: string
}

/** Carga de `install:progress` mientras se agrega un paquete (menú Agregar). */
export interface InstallProgress {
  id: string
  label: string
  step: string
  state: ProgressState
  detail?: string
}

export interface InstalledVersion {
  service: ServiceId
  version: string
  path: string
}

export type ThemePreference = 'system' | 'light' | 'dark'

export interface Settings {
  appName: string
  root: string
  defaultRoot: string
  theme: ThemePreference
  ssl: boolean
  /** Dominio de los sitios (`test`). */
  tld: string
  /** Motor de base de datos elegido. */
  database: 'mariadb' | 'mysql'
  hasRootPassword: boolean
}

/** `settings_get_ports` / `settings_set_ports`. */
export interface PortSettings {
  http: number
  https: number
  /** Primer puerto de PHP (en Windows, uno por cada php-cgi). */
  php: number
  database: number
  postgresql: number
  redis: number
  mongodb: number
  mailpitSmtp: number
  mailpitHttp: number
  tld: string
}

/** Paneles que el menú pide abrir en la ventana (`ui:panel`). */
export type Panel = 'ports' | 'password' | 'profile'

export type AppErrorKind =
  | 'io'
  | 'config'
  | 'template'
  | 'portInUse'
  | 'binaryMissing'
  | 'vcRuntimeMissing'
  | 'process'
  | 'timeout'
  | 'hosts'
  | 'invalidInput'
  | 'notFound'
  | 'unsupported'

/** Forma en que llega un `AppError` al rechazar un `invoke`. */
export interface AppError {
  kind: AppErrorKind
  message: string
}

export const EVENTS = {
  serviceState: 'service:state',
  sitesChanged: 'sites:changed',
  settingsTheme: 'settings:theme',
  settingsSsl: 'settings:ssl',
  sitesProgress: 'sites:progress',
  /** El menú contextual pide abrir el formulario de sitio nuevo. */
  uiCreateSite: 'ui:create-site',
  installProgress: 'install:progress',
  /** El menú pide abrir un panel (puertos, contraseña). */
  uiPanel: 'ui:panel',
  /** Cambió algo de Preferencias: volver a leer `settings_get`. */
  settingsChanged: 'settings:changed',
} as const
