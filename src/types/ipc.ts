// Espejo manual de los tipos de Rust (src-tauri/src/domain, error.rs y
// commands). Al cambiar uno, cambiar el otro en el mismo commit.

export type ServiceId = 'nginx' | 'php' | 'mariadb'

export type ServiceState = 'stopped' | 'starting' | 'running' | 'stopping' | 'failed'

/** Carga de `service:state` y elementos de `services_snapshot`. */
export interface ServiceStatus {
  id: ServiceId
  state: ServiceState
  detail?: string
  version: string | null
  port: number
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

export interface InstalledVersion {
  service: ServiceId
  version: string
  path: string
}

export interface Settings {
  appName: string
  root: string
  defaultRoot: string
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
} as const
