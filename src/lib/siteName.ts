// Nombre de un sitio nuevo. Espejo de `normalize_site_name` y `site_host`
// en src-tauri/src/domain/site.rs: si cambia uno, cambiar el otro.

const ACCENTS: Record<string, string> = {
  á: 'a', à: 'a', ä: 'a', â: 'a', ã: 'a',
  é: 'e', è: 'e', ë: 'e', ê: 'e',
  í: 'i', ì: 'i', ï: 'i', î: 'i',
  ó: 'o', ò: 'o', ö: 'o', ô: 'o', õ: 'o',
  ú: 'u', ù: 'u', ü: 'u', û: 'u',
  ñ: 'n', ç: 'c',
}

/**
 * Lo que se va escribiendo: minúsculas, sin tildes, los espacios como un
 * guion y sin otros caracteres. No recorta los extremos, para poder seguir
 * escribiendo después de un espacio ("mi " → "mi-").
 */
export function typingSiteName(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^\sa-z0-9_-]/g, (c) => ACCENTS[c] ?? '')
    .replace(/^\s+/, '')
    .replace(/\s+/g, '-')
    .slice(0, 63)
}

/** El nombre final: como `typingSiteName`, sin `-` ni `_` en los extremos. */
export function normalizeSiteName(input: string): string {
  return typingSiteName(input.trim()).replace(/^[-_]+|[-_]+$/g, '')
}

/** Parte del dominio: `_` no vale en un dominio (`mi_tienda` → `mi-tienda`). */
export function siteHost(name: string): string {
  return name.toLowerCase().replace(/_/g, '-')
}

export const SITE_NAME_RE = /^[a-z0-9](?:[a-z0-9_-]{0,61}[a-z0-9])?$/
