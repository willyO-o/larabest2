// Genera, a partir del logo, lo que no sale de `npx tauri icon`:
// - src-tauri/icons/tray-template.rgba: silueta del logo para la barra de
//   menús de macOS (36x36 RGBA crudo, lo incrusta tray.rs).
// - src-tauri/icons/nsis-header.bmp (150x57) y nsis-sidebar.bmp (164x314):
//   el logo en el encabezado y el panel lateral del instalador de Windows.
//
// Uso (después de rasterizar con tauri icon, ver README → Desarrollo):
//   node scripts/brand-assets.mjs <logo-48.png> <logo-120.png> <tray-36.png>

import { readFileSync, writeFileSync } from 'node:fs'
import { inflateSync } from 'node:zlib'
import { fileURLToPath } from 'node:url'

const icons = fileURLToPath(new URL('../src-tauri/icons/', import.meta.url))
const [logo48, logo120, tray36] = process.argv.slice(2)
if (!tray36) {
  console.error('Uso: node scripts/brand-assets.mjs <logo-48.png> <logo-120.png> <tray-36.png>')
  process.exit(1)
}

/** PNG RGBA de 8 bits sin entrelazar (lo que genera tauri icon). */
function readPng(path) {
  const buf = readFileSync(path)
  let offset = 8
  let width = 0
  let height = 0
  const idat = []
  while (offset < buf.length) {
    const length = buf.readUInt32BE(offset)
    const type = buf.toString('ascii', offset + 4, offset + 8)
    const data = buf.subarray(offset + 8, offset + 8 + length)
    if (type === 'IHDR') {
      width = data.readUInt32BE(0)
      height = data.readUInt32BE(4)
      if (data[8] !== 8 || data[9] !== 6 || data[12] !== 0) {
        throw new Error(`${path}: se esperaba RGBA de 8 bits sin entrelazar`)
      }
    } else if (type === 'IDAT') {
      idat.push(data)
    }
    offset += 12 + length
  }
  const raw = inflateSync(Buffer.concat(idat))
  const stride = width * 4
  const out = Buffer.alloc(stride * height)
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)]
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1))
    for (let x = 0; x < stride; x++) {
      const a = x >= 4 ? out[y * stride + x - 4] : 0
      const b = y > 0 ? out[(y - 1) * stride + x] : 0
      const c = x >= 4 && y > 0 ? out[(y - 1) * stride + x - 4] : 0
      let value = line[x]
      if (filter === 1) value += a
      else if (filter === 2) value += b
      else if (filter === 3) value += (a + b) >> 1
      else if (filter === 4) {
        const p = a + b - c
        const pa = Math.abs(p - a)
        const pb = Math.abs(p - b)
        const pc = Math.abs(p - c)
        value += pa <= pb && pa <= pc ? a : pb <= pc ? b : c
      }
      out[y * stride + x] = value & 0xff
    }
  }
  return { width, height, data: out }
}

/** Lienzo RGB con fondo de color y el logo encima (mezcla por alfa). */
function canvas(width, height, [r, g, b]) {
  const data = Buffer.alloc(width * height * 3)
  for (let i = 0; i < width * height; i++) data.set([r, g, b], i * 3)
  return {
    width,
    height,
    data,
    draw(img, left, top) {
      for (let y = 0; y < img.height; y++) {
        for (let x = 0; x < img.width; x++) {
          const tx = left + x
          const ty = top + y
          if (tx < 0 || ty < 0 || tx >= width || ty >= height) continue
          const s = (y * img.width + x) * 4
          const alpha = img.data[s + 3] / 255
          const d = (ty * width + tx) * 3
          for (let k = 0; k < 3; k++) {
            data[d + k] = Math.round(img.data[s + k] * alpha + data[d + k] * (1 - alpha))
          }
        }
      }
    },
    fill(left, top, w, h, [cr, cg, cb]) {
      for (let y = top; y < top + h; y++) {
        for (let x = left; x < left + w; x++) data.set([cr, cg, cb], (y * width + x) * 3)
      }
    },
  }
}

/** BMP de 24 bits, filas de abajo hacia arriba (lo que pide NSIS). */
function writeBmp(path, img) {
  const rowSize = Math.ceil((img.width * 3) / 4) * 4
  const size = 54 + rowSize * img.height
  const buf = Buffer.alloc(size)
  buf.write('BM', 0, 'ascii')
  buf.writeUInt32LE(size, 2)
  buf.writeUInt32LE(54, 10)
  buf.writeUInt32LE(40, 14)
  buf.writeInt32LE(img.width, 18)
  buf.writeInt32LE(img.height, 22)
  buf.writeUInt16LE(1, 26)
  buf.writeUInt16LE(24, 28)
  buf.writeUInt32LE(rowSize * img.height, 34)
  for (let y = 0; y < img.height; y++) {
    const row = 54 + (img.height - 1 - y) * rowSize
    for (let x = 0; x < img.width; x++) {
      const s = (y * img.width + x) * 3
      buf[row + x * 3] = img.data[s + 2]
      buf[row + x * 3 + 1] = img.data[s + 1]
      buf[row + x * 3 + 2] = img.data[s]
    }
  }
  writeFileSync(path, buf)
}

const WHITE = [255, 255, 255]
const BLUE = [37, 99, 235]

const tray = readPng(tray36)
writeFileSync(`${icons}tray-template.rgba`, tray.data)

const small = readPng(logo48)
const header = canvas(150, 57, WHITE)
header.draw(small, 150 - small.width - 6, Math.round((57 - small.height) / 2))
writeBmp(`${icons}nsis-header.bmp`, header)

const big = readPng(logo120)
const sidebar = canvas(164, 314, WHITE)
sidebar.draw(big, Math.round((164 - big.width) / 2), 56)
sidebar.fill(0, 310, 164, 4, BLUE)
writeBmp(`${icons}nsis-sidebar.bmp`, sidebar)

console.log(`tray-template.rgba (${tray.width}x${tray.height}), nsis-header.bmp, nsis-sidebar.bmp`)
