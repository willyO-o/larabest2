/** Extensiones que el navegador abre siempre por HTTPS (lista HSTS). */
const HSTS = ['app', 'dev', 'page', 'new', 'foo', 'day']

/**
 * Lo mismo que valida el núcleo (`domain::database::normalize_tld`), para
 * avisar mientras se escribe. `null` si sirve.
 */
export function tldProblem(value: string): string | null {
  const tld = value.trim().replace(/^\.+/, '').toLowerCase()
  if (!tld) return 'Escribe la extensión de los dominios, por ejemplo test.'
  if (tld.length > 24) return 'La extensión puede tener hasta 24 caracteres.'
  if (!/^[a-z][a-z0-9-]*$/.test(tld) || tld.endsWith('-')) {
    return 'La extensión empieza con una letra y lleva solo letras sin tilde, números y guiones.'
  }
  if (HSTS.includes(tld)) {
    return `.${tld} no sirve: los navegadores la abren siempre por HTTPS. Usa test, localhost o lab.`
  }
  if (tld === 'local') {
    return '.local no sirve: macOS y muchas redes la usan para buscar equipos y los sitios tardan en abrir. Usa test.'
  }
  return null
}
