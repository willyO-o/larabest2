// Espejo manual de los tipos de Rust (src-tauri/src/domain, error.rs y
// commands). Al cambiar uno, cambiar el otro en el mismo commit.

export type ServiceId =
  | 'nginx'
  | 'apache'
  | 'php'
  | 'mariadb'
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

export type SitePreset = 'blank' | 'wordpress' | 'laravel'

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
}

export type WebServer = 'nginx' | 'apache'

/** Puertos de Preferencias → Servicios y puertos (`PortSettings` en Rust). */
export interface PortSettings {
  http: number
  https: number
  /** Primer puerto FastCGI; PHP usa `phpPortCount` seguidos. */
  php: number
  mariadb: number
  postgresql: number
  redis: number
  mongodb: number
  mailpitSmtp: number
  mailpitHttp: number
}

/** Resultado de `services_settings_get`. */
export interface ServicesSettings {
  webServer: WebServer
  ports: PortSettings
  phpPortCount: number
}

export interface ServiceToggle {
  id: ServiceId
  enabled: boolean
}

/** Lo que guarda `services_settings_save`. */
export interface ServicesChoice {
  webServer: WebServer
  /** Solo servicios que se pueden quitar: MariaDB y los opcionales. */
  enabled: ServiceToggle[]
  ports: PortSettings
}

/** Un puerto que ya ocupa otro programa (`services_settings_busy`). */
export interface PortBusy {
  port: number
  program: string
}

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
  /** El menú contextual pide abrir Servicios y puertos. */
  uiServices: 'ui:services',
  installProgress: 'install:progress',
} as const
