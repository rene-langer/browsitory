#!/usr/bin/env node
/**
 * Generates placeholder PWA icons (public/icon-*.png) with zero runtime or
 * devDependency footprint — hand-rolled PNG encoding via Node's built-in
 * `zlib`, no canvas/sharp/etc.
 *
 * These are intentionally simple placeholders (solid brand-purple square +
 * blocky "B" monogram), not final branding. Re-run with `node
 * scripts/generate-icons.mjs` any time the source bitmap/colors below
 * change; nothing else in the repo depends on this script existing.
 */
import { deflateSync } from 'node:zlib'
import { writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const __dirname = dirname(fileURLToPath(import.meta.url))
const publicDir = join(__dirname, '..', 'public')

// Brand purple, matching src/styles/globals.css's --primary (hsl(262.1 80% 50.6%)).
const BG = [0x7c, 0x3a, 0xed, 0xff]
const FG = [0xff, 0xff, 0xff, 0xff]

// 5x7 blocky "B" glyph (1 = foreground pixel), classic dot-matrix style.
const GLYPH = ['11110', '10001', '10001', '11110', '10001', '10001', '11110']

// --- Minimal PNG encoder (RGBA8, filter type 0/None per scanline) ---

const CRC_TABLE = (() => {
  const table = new Uint32Array(256)
  for (let n = 0; n < 256; n++) {
    let c = n
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
    }
    table[n] = c >>> 0
  }
  return table
})()

function crc32(buf) {
  let c = 0xffffffff
  for (let i = 0; i < buf.length; i++) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8)
  }
  return (c ^ 0xffffffff) >>> 0
}

function chunk(type, data) {
  const typeBuf = Buffer.from(type, 'ascii')
  const lenBuf = Buffer.alloc(4)
  lenBuf.writeUInt32BE(data.length, 0)
  const crcBuf = Buffer.alloc(4)
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0)
  return Buffer.concat([lenBuf, typeBuf, data, crcBuf])
}

function encodePNG(width, height, rgba) {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])

  const ihdrData = Buffer.alloc(13)
  ihdrData.writeUInt32BE(width, 0)
  ihdrData.writeUInt32BE(height, 4)
  ihdrData[8] = 8 // bit depth
  ihdrData[9] = 6 // color type: RGBA
  ihdrData[10] = 0 // compression
  ihdrData[11] = 0 // filter
  ihdrData[12] = 0 // interlace
  const ihdr = chunk('IHDR', ihdrData)

  // Raw scanlines: 1 filter byte (None) + width*4 RGBA bytes, per row.
  const raw = Buffer.alloc(height * (1 + width * 4))
  for (let y = 0; y < height; y++) {
    const rowStart = y * (1 + width * 4)
    raw[rowStart] = 0 // filter: None
    rgba.copy(raw, rowStart + 1, y * width * 4, (y + 1) * width * 4)
  }
  const idat = chunk('IDAT', deflateSync(raw))

  const iend = chunk('IEND', Buffer.alloc(0))

  return Buffer.concat([signature, ihdr, idat, iend])
}

// --- Icon drawing ---

/**
 * @param {number} size square icon size in px
 * @param {number} safeFraction fraction of `size` the glyph's bounding box
 *   should occupy — kept <= 0.8 for maskable icons per the maskable-icon
 *   safe-zone spec (content within the inner ~80%, background full-bleed).
 */
function drawIcon(size, safeFraction) {
  const rgba = Buffer.alloc(size * size * 4)
  for (let i = 0; i < size * size; i++) {
    rgba.set(BG, i * 4)
  }

  const cols = GLYPH[0].length
  const rows = GLYPH.length
  const box = size * safeFraction
  const scale = Math.max(1, Math.floor(Math.min(box / cols, box / rows)))
  const glyphW = cols * scale
  const glyphH = rows * scale
  const offsetX = Math.floor((size - glyphW) / 2)
  const offsetY = Math.floor((size - glyphH) / 2)

  for (let row = 0; row < rows; row++) {
    for (let col = 0; col < cols; col++) {
      if (GLYPH[row][col] !== '1') continue
      for (let py = 0; py < scale; py++) {
        for (let px = 0; px < scale; px++) {
          const x = offsetX + col * scale + px
          const y = offsetY + row * scale + py
          const idx = (y * size + x) * 4
          rgba.set(FG, idx)
        }
      }
    }
  }

  return encodePNG(size, size, rgba)
}

const targets = [
  { file: 'icon-192.png', size: 192, safeFraction: 0.6 },
  { file: 'icon-512.png', size: 512, safeFraction: 0.6 },
  { file: 'icon-192-maskable.png', size: 192, safeFraction: 0.8 },
  { file: 'icon-512-maskable.png', size: 512, safeFraction: 0.8 },
]

for (const { file, size, safeFraction } of targets) {
  const png = drawIcon(size, safeFraction)
  writeFileSync(join(publicDir, file), png)
  console.log(`wrote public/${file} (${size}x${size}, ${png.length} bytes)`)
}
