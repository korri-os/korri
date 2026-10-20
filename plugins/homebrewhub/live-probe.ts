#!/usr/bin/env -S nix shell nixpkgs#bun --command bun
// Live probe for @korri:homebrewhub. Not part of the unit suite.
//
// Proves each pinned artifact is still what plugin.ts says: it GETs every
// artifact file (each is small) and compares status, size and SHA-256 with the
// pin, checks the Game Boy header logo at 0x104, and checks that Homebrew
// Hub's API record still lists the file. One timestamped line per step.
// GETs only, no auth.
//
// Usage: plugins/homebrewhub/live-probe.ts [--record <dir>]
// --record rewrites the committed API record fixtures.
import { createHash } from "node:crypto"
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { artifacts, games } from "./plugin"

// First 16 bytes of the Nintendo logo at 0x104 in a Game Boy cartridge header.
const GB_LOGO = { offset: 0x104, hex: "ceed6666cc0d000b03730083000c000d" }

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

for (const artifact of Object.values(artifacts)) {
  for (const pin of artifact.files) {
    const response = await fetch(pin.url, { redirect: "manual" })
    const bytes = Buffer.from(await response.arrayBuffer())
    const sha256 = `sha256:${createHash("sha256").update(bytes).digest("hex")}`
    log(`GET ${pin.url} -> ${response.status} ${bytes.length} bytes ${sha256}`)
    check(response.status === 200, `${artifact.id}: answers 200 without a redirect`)
    check(bytes.length === pin.bytes && sha256 === pin.sha256, `${artifact.id}: size and SHA-256 match the pin`)
    const logo = bytes.subarray(GB_LOGO.offset, GB_LOGO.offset + 16).toString("hex")
    check(logo === GB_LOGO.hex, `${artifact.id}: carries the Game Boy header logo (${logo})`)
  }
}

for (const [key, game] of Object.entries(games)) {
  const url = `https://hh3.gbdev.io/api/entry/${key}.json`
  const response = await fetch(url)
  const text = await response.text()
  log(`GET ${url} -> ${response.status} ${text.length} chars`)
  let files: string[] = []
  try {
    files = (JSON.parse(text).files ?? []).map((file: { filename: string }) => file.filename)
  } catch {}
  for (const release of Object.values(game.releases)) {
    check(response.ok && files.includes(release.file.path), `${key}: API record lists ${release.file.path}`)
  }
  if (recordDir && response.ok) {
    writeFileSync(join(recordDir, `${key}.json`), text.endsWith("\n") ? text : `${text}\n`)
    log(`recorded ${key}.json`)
  }
}

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
