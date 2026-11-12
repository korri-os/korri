/**
 * Korri's identity backup as a QR code image. The code is encoded here
 * (encodeQr, below); the image is a PNG written here, one pixel a module
 * with the four-module quiet zone, so the shell decodes it once (contract
 * 3.8) and Slint scales it with `image-rendering: pixelated`. No canvas, no
 * DOM. The encoder is the one Zone's package holds (packages/zone/src/qr.ts),
 * copied: a package imports nothing from another.
 */
import { onForget } from "./memo"
/*
 * A QR code in byte mode at error correction level M with mask 0, the
 * smallest version that holds the text (ISO/IEC 18004; the steps follow
 * Project Nayuki's qrcodegen). Zone used the `qrcode` package; its modules
 * cost the shell's 50 ms load budget a third of Zone's load, so the few
 * steps Zone needs are written here. test/qr.test.ts holds the modules
 * equal to the `qrcode` package's for the same text, level and mask.
 */
const ECC_M = [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28]
const BLOCKS_M = [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49]

function rawModules(ver: number): number {
  let result = (16 * ver + 128) * ver + 64
  if (ver >= 2) {
    const align = Math.floor(ver / 7) + 2
    result -= (25 * align - 10) * align - 55
    if (ver >= 7) result -= 36
  }
  return result
}

const dataCodewords = (ver: number): number => Math.floor(rawModules(ver) / 8) - ECC_M[ver]! * BLOCKS_M[ver]!

function gfMultiply(x: number, y: number): number {
  let z = 0
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d)
    z ^= ((y >>> i) & 1) * x
  }
  return z & 0xff
}

function rsDivisor(degree: number): number[] {
  const result = new Array<number>(degree).fill(0)
  result[degree - 1] = 1
  let root = 1
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      result[j] = gfMultiply(result[j]!, root)
      if (j + 1 < degree) result[j] = result[j]! ^ result[j + 1]!
    }
    root = gfMultiply(root, 0x02)
  }
  return result
}

function rsRemainder(data: readonly number[], divisor: readonly number[]): number[] {
  const result = divisor.map(() => 0)
  for (const b of data) {
    const factor = b ^ result.shift()!
    result.push(0)
    divisor.forEach((coef, i) => (result[i] = result[i]! ^ gfMultiply(coef, factor)))
  }
  return result
}

function alignments(ver: number, size: number): number[] {
  if (ver === 1) return []
  const count = Math.floor(ver / 7) + 2
  const step = ver === 32 ? 26 : Math.ceil((ver * 4 + 4) / (count * 2 - 2)) * 2
  const result = [6]
  for (let pos = size - 7; result.length < count; pos -= step) result.splice(1, 0, pos)
  return result
}

const bit = (x: number, i: number): boolean => ((x >>> i) & 1) !== 0

/** The modules, row by row (true is dark), and the side. */
export function encodeQr(bytes: readonly number[]): { readonly size: number; readonly dark: readonly boolean[] } {
  let ver = 1
  for (; ver <= 40; ver++) if (4 + (ver <= 9 ? 8 : 16) + bytes.length * 8 <= dataCodewords(ver) * 8) break
  if (ver > 40) throw new RangeError("too long for a QR code")
  const capacity = dataCodewords(ver)
  // The data bits: mode, count, bytes, terminator, padding.
  const bits: number[] = []
  const put = (value: number, length: number) => {
    for (let i = length - 1; i >= 0; i--) bits.push((value >>> i) & 1)
  }
  put(4, 4)
  put(bytes.length, ver <= 9 ? 8 : 16)
  for (const b of bytes) put(b, 8)
  put(0, Math.min(4, capacity * 8 - bits.length))
  put(0, (8 - (bits.length % 8)) % 8)
  for (let pad = 0xec; bits.length < capacity * 8; pad ^= 0xec ^ 0x11) put(pad, 8)
  const data: number[] = []
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((acc, b) => (acc << 1) | b, 0))
  // The blocks, each with its error correction, interleaved.
  const blocks = BLOCKS_M[ver]!
  const ecc = ECC_M[ver]!
  const raw = Math.floor(rawModules(ver) / 8)
  const short = blocks - (raw % blocks)
  const shortLength = Math.floor(raw / blocks)
  const divisor = rsDivisor(ecc)
  const split: number[][] = []
  for (let i = 0, k = 0; i < blocks; i++) {
    const part = data.slice(k, k + shortLength - ecc + (i < short ? 0 : 1))
    k += part.length
    const withEcc = [...part, ...rsRemainder(part, divisor)]
    if (i < short) withEcc.splice(part.length, 0, -1)
    split.push(withEcc)
  }
  const codewords: number[] = []
  for (let i = 0; i < split[0]!.length; i++) {
    for (const block of split) if (block[i] !== -1) codewords.push(block[i]!)
  }
  // The matrix.
  const size = ver * 4 + 17
  const dark = new Array<boolean>(size * size).fill(false)
  const fixed = new Array<boolean>(size * size).fill(false)
  const set = (x: number, y: number, on: boolean) => {
    dark[y * size + x] = on
    fixed[y * size + x] = true
  }
  for (let i = 0; i < size; i++) {
    set(6, i, i % 2 === 0)
    set(i, 6, i % 2 === 0)
  }
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]] as const) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx
        const y = cy + dy
        const dist = Math.max(Math.abs(dx), Math.abs(dy))
        if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, dist !== 2 && dist !== 4)
      }
    }
  }
  const positions = alignments(ver, size)
  const last = positions.length - 1
  positions.forEach((ax, i) => positions.forEach((ay, j) => {
    if ((i === 0 && j === 0) || (i === 0 && j === last) || (i === last && j === 0)) return
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(ax + dx, ay + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1)
  }))
  // Format bits: level M (0) and mask 0.
  let rem = 0
  for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537)
  const format = ((0 << 10) | rem) ^ 0x5412
  for (let i = 0; i <= 5; i++) set(8, i, bit(format, i))
  set(8, 7, bit(format, 6))
  set(8, 8, bit(format, 7))
  set(7, 8, bit(format, 8))
  for (let i = 9; i < 15; i++) set(14 - i, 8, bit(format, i))
  for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(format, i))
  for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(format, i))
  set(8, size - 8, true)
  if (ver >= 7) {
    let vrem = ver
    for (let i = 0; i < 12; i++) vrem = (vrem << 1) ^ ((vrem >>> 11) * 0x1f25)
    const version = (ver << 12) | vrem
    for (let i = 0; i < 18; i++) {
      const a = size - 11 + (i % 3)
      const b = Math.floor(i / 3)
      set(a, b, bit(version, i))
      set(b, a, bit(version, i))
    }
  }
  // The codewords, two columns at a time, up and down; mask 0 on the way.
  let i = 0
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5
    for (let vert = 0; vert < size; vert++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j
        const upward = ((right + 1) & 2) === 0
        const y = upward ? size - 1 - vert : vert
        if (fixed[y * size + x]) continue
        const on = i < codewords.length * 8 && bit(codewords[i >>> 3]!, 7 - (i & 7))
        if (i < codewords.length * 8) i++
        dark[y * size + x] = on !== ((x + y) % 2 === 0)
      }
    }
  }
  return { size, dark }
}

const QUIET = 4

/** The CRC-32 table: made at the first code (a backup is rare), and the same every time it is made. */
let crcTable: Uint32Array | undefined
function crc32(bytes: Uint8Array): number {
  if (crcTable === undefined) {
    crcTable = new Uint32Array(256)
    for (let n = 0; n < 256; n++) {
      let c = n
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
      crcTable[n] = c >>> 0
    }
  }
  let crc = 0xffffffff
  for (const byte of bytes) crc = crcTable[(crc ^ byte) & 0xff]! ^ (crc >>> 8)
  return (crc ^ 0xffffffff) >>> 0
}

const be32 = (out: Uint8Array, at: number, value: number) => {
  out[at] = (value >>> 24) & 0xff
  out[at + 1] = (value >>> 16) & 0xff
  out[at + 2] = (value >>> 8) & 0xff
  out[at + 3] = value & 0xff
}

/** One PNG chunk: length, type, data, CRC over type and data. */
function chunk(type: string, data: Uint8Array): Uint8Array {
  const out = new Uint8Array(12 + data.length)
  be32(out, 0, data.length)
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i)
  out.set(data, 8)
  be32(out, 8 + data.length, crc32(out.subarray(4, 8 + data.length)))
  return out
}

/** zlib with stored (uncompressed) deflate blocks: the image is a few KB. */
function zlibStored(raw: Uint8Array): Uint8Array {
  const blocks = Math.max(1, Math.ceil(raw.length / 65535))
  const out = new Uint8Array(2 + blocks * 5 + raw.length + 4)
  out[0] = 0x78
  out[1] = 0x01
  let at = 2
  for (let block = 0; block < blocks; block++) {
    const part = raw.subarray(block * 65535, Math.min(raw.length, (block + 1) * 65535))
    out[at] = block === blocks - 1 ? 1 : 0
    out[at + 1] = part.length & 0xff
    out[at + 2] = part.length >> 8
    out[at + 3] = ~part.length & 0xff
    out[at + 4] = (~part.length >> 8) & 0xff
    out.set(part, at + 5)
    at += 5 + part.length
  }
  let a = 1
  let b = 0
  for (let i = 0; i < raw.length; i++) {
    a = (a + raw[i]!) % 65521
    b = (b + a) % 65521
  }
  be32(out, at, ((b << 16) | a) >>> 0)
  return out
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
function base64(bytes: Uint8Array): string {
  const parts: string[] = []
  for (let i = 0; i < bytes.length; i += 3) {
    const n = (bytes[i]! << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0)
    parts.push(BASE64[(n >> 18) & 63]! + BASE64[(n >> 12) & 63]! + (i + 1 < bytes.length ? BASE64[(n >> 6) & 63]! : "=") + (i + 2 < bytes.length ? BASE64[n & 63]! : "="))
  }
  return parts.join("")
}

function utf8(text: string): number[] {
  const out: number[] = []
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0
    if (code < 0x80) out.push(code)
    else if (code < 0x800) out.push(0xc0 | (code >> 6), 0x80 | (code & 63))
    else if (code < 0x10000) out.push(0xe0 | (code >> 12), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63))
    else out.push(0xf0 | (code >> 18), 0x80 | ((code >> 12) & 63), 0x80 | ((code >> 6) & 63), 0x80 | (code & 63))
  }
  return out
}

// The code of a text, by the whole text: a view of the backup page asks for the same one again.
const cache = new Map<string, string>()
onForget(() => {
  cache.clear()
  crcTable = undefined
})

/** A `data:image/png` URL of the code for `text`, black on white. */
export function qrDataUrl(text: string): string {
  const known = cache.get(text)
  if (known !== undefined) return known
  // One byte segment of the text's UTF-8 (the shell's engine has no
  // TextEncoder); Korri's backup is bech32, so its characters are its bytes.
  const code = encodeQr(utf8(text))
  const size = code.size + 2 * QUIET
  // Grey, one byte a pixel, each row led by filter 0.
  const raw = new Uint8Array(size * (size + 1))
  for (let y = 0; y < size; y++) {
    raw[y * (size + 1)] = 0
    for (let x = 0; x < size; x++) {
      const mx = x - QUIET
      const my = y - QUIET
      const dark = mx >= 0 && my >= 0 && mx < code.size && my < code.size && code.dark[my * code.size + mx] === true
      raw[y * (size + 1) + 1 + x] = dark ? 0 : 255
    }
  }
  const header = new Uint8Array(13)
  be32(header, 0, size)
  be32(header, 4, size)
  header[8] = 8
  const parts = [new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), chunk("IHDR", header), chunk("IDAT", zlibStored(raw)), chunk("IEND", new Uint8Array())]
  const png = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0))
  let at = 0
  for (const part of parts) {
    png.set(part, at)
    at += part.length
  }
  const url = `data:image/png;base64,${base64(png)}`
  cache.clear()
  cache.set(text, url)
  return url
}
