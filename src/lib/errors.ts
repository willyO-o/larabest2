import { ref } from 'vue'
import type { AppError } from '../types/ipc'

/** Último error de una acción, para mostrarlo sin diálogos modales. */
export const lastError = ref<string | null>(null)

export function report(e: unknown) {
  const err = e as Partial<AppError> | string
  lastError.value = typeof err === 'string' ? err : (err?.message ?? String(e))
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
