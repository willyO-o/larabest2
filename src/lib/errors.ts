import { ref } from 'vue'
import type { AppError } from '../types/ipc'

/** Último error de una acción, para mostrarlo sin diálogos modales. */
export const lastError = ref<string | null>(null)

/** Texto de un error de `invoke` (un `AppError`) o de cualquier otro. */
export function messageOf(e: unknown): string {
  const err = e as Partial<AppError> | string
  return typeof err === 'string' ? err : (err?.message ?? String(e))
}

export function report(e: unknown) {
  lastError.value = messageOf(e)
}

/** Ejecuta un `invoke` y guarda el error si falla. */
export async function attempt<T>(fn: () => Promise<T>): Promise<T | undefined> {
  try {
    lastError.value = null
    return await fn()
  } catch (e) {
    report(e)
    return undefined
  }
}
