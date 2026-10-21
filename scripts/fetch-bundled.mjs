// Descarga los programas que el instalador de Windows trae dentro (HeidiSQL
// y Notepad++) en src-tauri/resources/bundled/. La app los extrae en bin/
// al abrirse. Versiones fijas y verificadas con SHA-256: para actualizar
// una, cambiar su versión, URL y hash (y probarla antes de publicar).
//
//   npm run bundled
//
// Lo llama `npm run build:windows` antes de compilar.

import { createHash } from 'node:crypto'
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const TOOLS = [
  {
    key: 'heidisql',
    label: 'HeidiSQL',
    version: '12.21',
    url: 'https://github.com/HeidiSQL/HeidiSQL/releases/download/v12.21/HeidiSQL_12.21_64_Portable.zip',
    sha256: 'fecb76a69e29a53ea05b1d57fc2f7b7aaed5b8f889556c6eca545e2a800df1ab',
    license: 'GNU GPL',
    source: 'https://github.com/HeidiSQL/HeidiSQL',
  },
  {
    key: 'notepad++',
    label: 'Notepad++',
    version: '8.9.8.1',
    url: 'https://github.com/notepad-plus-plus/notepad-plus-plus/releases/download/v8.9.8.1/npp.8.9.8.1.portable.x64.zip',
    sha256: 'PENDIENTE',
    license: 'GNU GPL v2',
    source: 'https://github.com/notepad-plus-plus/notepad-plus-plus',
  },
]

const LICENSES = 'LICENCIAS.txt'
const dir = fileURLToPath(new URL('../src-tauri/resources/bundled/', import.meta.url))

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex')

async function download(url) {
  let lastError
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(url, { redirect: 'follow' })
      if (!res.ok) throw new Error(`HTTP ${res.status}`)
      return Buffer.from(await res.arrayBuffer())
    } catch (e) {
      lastError = e
      console.warn(`  intento ${attempt} falló: ${e.message}`)
      await new Promise((r) => setTimeout(r, 2000 * attempt))
    }
  }
  throw new Error(`No se pudo descargar ${url}: ${lastError?.message}`)
}

await mkdir(dir, { recursive: true })
const wanted = new Set([LICENSES])

for (const tool of TOOLS) {
  const file = `${tool.key}-${tool.version}.zip`
  const path = join(dir, file)
  wanted.add(file)
  const existing = await readFile(path).catch(() => null)
  if (existing && sha256(existing) === tool.sha256) {
    console.log(`${tool.label} ${tool.version}: ya descargado`)
    continue
  }
  console.log(`${tool.label} ${tool.version}: descargando ${tool.url}`)
  const bytes = await download(tool.url)
  const digest = sha256(bytes)
  if (digest !== tool.sha256) {
    throw new Error(
      `${tool.label}: el SHA-256 no coincide.\n  esperado ${tool.sha256}\n  recibido ${digest}`,
    )
  }
  await writeFile(path, bytes)
  console.log(`${tool.label} ${tool.version}: listo (${(bytes.length / 1e6).toFixed(1)} MB)`)
}

// Versiones anteriores que quedaron en la carpeta.
for (const name of await readdir(dir)) {
  if (!wanted.has(name)) await rm(join(dir, name), { force: true })
}

const notice = [
  'Programas de terceros incluidos en el instalador de LARABEST',
  '',
  'Se distribuyen sin modificar, en su versión portable oficial. Cada zip trae',
  'su propio archivo de licencia. El código fuente de cada versión está en su',
  'repositorio:',
  '',
  ...TOOLS.map((t) => `- ${t.label} ${t.version} (${t.license}): ${t.source} (etiqueta v${t.version})`),
  '',
].join('\r\n')
await writeFile(join(dir, LICENSES), notice)
