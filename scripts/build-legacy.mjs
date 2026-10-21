// Instalador "legacy" para Windows 7 SP1, 8, 8.1 y Server 2008 R2/2012/
// 2012 R2. Rust estable exige Windows 10 desde la 1.78; para los anteriores
// existe el target x86_64-win7-windows-msvc (Tier 3, sin binarios
// precompilados), que se compila con nightly y -Z build-std.
//
//   rustup toolchain install nightly --component rust-src
//   npm run build:legacy
//
// Pasos:
// 1. Compila con Visual Studio dos DLL de compatibilidad (scripts/legacy-shims):
//    combase.dll (Windows 7 no lo tiene; reenvía a ole32.dll) y
//    api-ms-win-core-winrt-error-l1-1-0.dll (WinRT, que Windows 7 no tiene).
//    Van junto al .exe; Windows 8 o posterior usa las suyas y las ignora.
// 2. Compila la app y el instalador para el target de Windows 7 con ellas.
// 3. Comprueba que todo lo que el .exe importa de esas DLL está cubierto.
// 4. Deja src-tauri/target/legacy/release/bundle/nsis/
//    LARABEST_<versión>_x64-legacy-setup.exe.
//
// Tauri no acepta el target (Tier 3, no está en `rustup target list`): se le
// da un "runner" (scripts/legacy-cargo.cmd) que compila para Windows 7 y deja
// el .exe donde Tauri lo empaqueta. Todo va en target/legacy, aparte del
// build normal.

import { spawnSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('..', import.meta.url))
const tauriDir = join(root, 'src-tauri')
const shimsSrc = join(root, 'scripts', 'legacy-shims')
const shimsOut = join(tauriDir, 'target', 'legacy-shims')
const targetDir = join(tauriDir, 'target', 'legacy')
const runner = join(root, 'scripts', 'legacy-cargo.cmd')
const SHIMS = [
  { def: 'combase.def', dll: 'combase.dll' },
  { def: 'winrt-error.def', dll: 'api-ms-win-core-winrt-error-l1-1-0.dll' },
]

function fail(message) {
  console.error(`\nbuild:legacy: ${message}`)
  process.exit(1)
}

/** Carpeta de Visual Studio con las herramientas de C++. */
function visualStudio() {
  const vswhere = join(
    process.env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)',
    'Microsoft Visual Studio',
    'Installer',
    'vswhere.exe',
  )
  if (!existsSync(vswhere)) fail('No se encontró vswhere.exe: instala las Build Tools de Visual Studio con C++.')
  const out = spawnSync(
    vswhere,
    ['-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'],
    { encoding: 'utf8' },
  )
  const path = out.stdout.trim().split(/\r?\n/)[0]
  if (!path) fail('No hay herramientas de C++ de Visual Studio (cl.exe, link.exe).')
  return path
}

const vcvars = join(visualStudio(), 'VC', 'Auxiliary', 'Build', 'vcvars64.bat')

/** Ejecuta un comando con el entorno de Visual Studio (cl, link, dumpbin). */
function vc(command, cwd) {
  const out = spawnSync('cmd.exe', ['/d', '/s', '/c', `"call "${vcvars}" >nul && ${command}"`], {
    cwd,
    encoding: 'utf8',
    windowsVerbatimArguments: true,
    maxBuffer: 64 * 1024 * 1024,
  })
  if (out.status !== 0) fail(`Falló: ${command}\n${out.stdout}\n${out.stderr}`)
  return out.stdout
}

/** Exportaciones de un .def: `[nombre, destino]` (destino `ole32.X` reenvía). */
function exportsOf(def) {
  const text = readFileSync(join(shimsSrc, def), 'utf8')
  const body = text.split(/^EXPORTS\s*$/m)[1] ?? ''
  return body
    .split(/\r?\n/)
    .map((l) => l.replace(/;.*/, '').trim())
    .filter(Boolean)
    .map((l) => l.split('=').map((part) => part.trim()))
}

// 1. DLL de compatibilidad. Los .def son la lista; las exportaciones se
// declaran con #pragma en C porque así link.exe sí crea los reenvíos a
// ole32 (con /DEF los toma como símbolos propios y falla).
mkdirSync(shimsOut, { recursive: true })
const zero = readFileSync(join(shimsSrc, 'zero.c'), 'utf8')
for (const shim of SHIMS) {
  const name = shim.dll.replace(/\.dll$/i, '')
  const pragmas = exportsOf(shim.def)
    .map(([exported, target]) => `#pragma comment(linker, "/EXPORT:${exported}=${target}")`)
    .join('\n')
  writeFileSync(join(shimsOut, `${name}.c`), `${pragmas}\n${zero}`)
  vc(`cl /nologo /c /O1 /GS- /Zl "${name}.c" /Fo"${name}.obj"`, shimsOut)
  vc(`link /nologo /DLL /NOENTRY /NODEFAULTLIB /MACHINE:X64 /OUT:"${shim.dll}" "${name}.obj"`, shimsOut)
  console.log(`DLL de compatibilidad: ${shim.dll}`)
}
const config = join(shimsOut, 'tauri.legacy.conf.json')
writeFileSync(
  config,
  JSON.stringify({
    bundle: {
      resources: Object.fromEntries(SHIMS.map((s) => [`target/legacy-shims/${s.dll}`, s.dll])),
    },
  }),
)

// 2. App e instalador para Windows 7.
mkdirSync(join(targetDir, 'release'), { recursive: true })
const build = spawnSync(
  'npx',
  ['tauri', 'build', '--runner', `"${runner}"`, '--config', `"${config}"`],
  { cwd: root, stdio: 'inherit', shell: true, env: { ...process.env, CARGO_TARGET_DIR: targetDir } },
)
if (build.status !== 0) process.exit(build.status ?? 1)

// 3. Lo que el .exe importa de esas DLL tiene que estar en ellas.
const releaseDir = join(targetDir, 'release')
const imports = vc(`dumpbin /nologo /imports "${join(releaseDir, 'larabest.exe')}"`, releaseDir)
let current = null
const missing = []
for (const line of imports.split(/\r?\n/)) {
  const dll = line.match(/^\s+(\S+\.dll)$/i)
  if (dll) {
    current = SHIMS.find((s) => s.dll.toLowerCase() === dll[1].toLowerCase()) ?? null
    continue
  }
  const fn = line.match(/^\s+[0-9A-F]+\s+(\S+)$/)
  const covered = current && exportsOf(current.def).some(([exported]) => exported === fn?.[1])
  if (current && fn && !covered) missing.push(`${current.dll}!${fn[1]}`)
}
if (missing.length) {
  fail(`el .exe importa funciones que las DLL de compatibilidad no cubren (agrégalas en scripts/legacy-shims):\n  ${missing.join('\n  ')}`)
}
console.log('Importaciones de Windows 8+ cubiertas por las DLL de compatibilidad.')

// 4. Nombre propio para no confundirlo con el instalador normal.
const nsis = join(releaseDir, 'bundle', 'nsis')
for (const name of readdirSync(nsis)) {
  if (!name.endsWith('-setup.exe') || name.includes('-legacy-')) continue
  const legacy = name.replace('-setup.exe', '-legacy-setup.exe')
  copyFileSync(join(nsis, name), join(nsis, legacy))
  rmSync(join(nsis, name))
  console.log(`Instalador legacy: ${join(nsis, legacy)}`)
}
