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
  /** Durante una descarga: bytes descargados... */
  downloaded?: number
  /** ...del total, si el servidor lo informa... */
  total?: number
  /** ...y la velocidad en bytes por segundo. */
  speed?: number
}

/** Cómo está compilado un PHP (`php_build_info`, `PhpBuild` en Rust). */
export interface PhpBuild {
  version: string
  /** false = NTS (Non Thread Safe), lo que usa la app con FastCGI. */
  threadSafe: boolean
  /** "x64"; macOS no lo informa. */
  arch: string | null
  /** "Visual C++ 2022". */
  compiler: string | null
  /** Compilador de las extensiones en Windows: "VS17", "VS16". */
  toolset: string | null
  /** "API420240924". */
  extensionApi: string | null
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
  /** Extensión de los dominios, sin punto. */
  tld: string
  /** Un dominio por carpeta de www; si no, `localhost/<carpeta>/`. */
  autoVhosts: boolean
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
  sites: SiteSettings
}

/** Hosts virtuales automáticos y extensión de los dominios. */
export interface SiteSettings {
  /** Un dominio `<carpeta>.<tld>` por carpeta de www; si no, `localhost/<carpeta>/`. */
  autoVhosts: boolean
  /** Sin punto: "test". */
  tld: string
}

/** Panel de bases de datos (`db_info`). */
export interface DatabaseInfo {
  running: boolean
  databases: string[]
  hasPassword: boolean
  backupsDir: string
}

/** Qué pide el menú MariaDB al abrir el panel. */
export type DatabaseMode = 'create' | 'backup' | 'password'

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
  sites: SiteSettings
}

/** Un puerto que ya ocupa otro programa (`services_settings_busy`). */
export interface PortBusy {
  port: number
  program: string
}

/** Un paquete del asistente "Preparar LARABEST" (`SetupPackage` en Rust). */
export interface SetupPackage {
  id: string
  label: string
  /** 0 la pila base, 1 los servicios opcionales, 2 las herramientas. */
  group: number
  /** Lo instalado ("8.4.26", "instalado"), o null. */
  status: string | null
  /** Marcado de entrada. */
  recommended: boolean
}

/** Gestor de paquetes de macOS (Homebrew o MacPorts). */
export interface SetupManager {
  name: string
  installed: boolean
}

/** Resultado de `setup_info`. */
export interface SetupInfo {
  /** Abrirlo al iniciar: todavía no se terminó ni se omitió. */
  show: boolean
  /** "Windows 11", "Windows 7", "macOS". */
  os: string
  /** En macOS, con qué se instala todo; null en Windows. */
  manager: SetupManager | null
  packages: SetupPackage[]
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
  /** El menú contextual pide abrir el asistente "Preparar LARABEST". */
  uiSetup: 'ui:setup',
  /** El menú MariaDB pide el panel de bases de datos (con su pestaña). */
  uiDatabase: 'ui:database',
  installProgress: 'install:progress',
} as const
