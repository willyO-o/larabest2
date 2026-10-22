import type { InstallProgress } from '../types/ipc'

const UNITS = ['B', 'KB', 'MB', 'GB']

/** 1536000 → "1,5 MB" (con coma decimal, como el resto de la interfaz). */
export function formatBytes(bytes: number): string {
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024
    unit++
  }
  const digits = unit === 0 || value >= 100 ? 0 : 1
  return `${value.toLocaleString('es', { maximumFractionDigits: digits, minimumFractionDigits: digits })} ${UNITS[unit]}`
}

/** Porcentaje de una descarga (0–100), o null si no se sabe el total. */
export function percentOf(p: InstallProgress | null | undefined): number | null {
  if (!p || p.downloaded == null || !p.total) return null
  return Math.min(100, Math.round((p.downloaded / p.total) * 100))
}

/**
 * "Descargando PHP 8.4.27 · 45 % · 2,1 MB/s", o el paso solo si no es una
 * descarga. Sin el total muestra lo descargado ("12,3 MB").
 */
export function progressText(p: InstallProgress): string {
  if (p.downloaded == null) return `${p.step}…`
  const parts = [p.step]
  const percent = percentOf(p)
  parts.push(percent != null ? `${percent} %` : formatBytes(p.downloaded))
  if (p.speed) parts.push(`${formatBytes(p.speed)}/s`)
  return parts.join(' · ')
}

/** "12,3 de 29,8 MB" para el título (al pasar el cursor). */
export function progressDetail(p: InstallProgress): string {
  if (p.downloaded == null) return p.detail ?? p.step
  return p.total
    ? `${formatBytes(p.downloaded)} de ${formatBytes(p.total)}`
    : formatBytes(p.downloaded)
}
