#!/usr/bin/env -S nix shell nixpkgs#bun --command bun
// Live probe for @korri:homebrewhub. Not part of the unit suite.
//
// The catalog is curated, so the handlers make no requests. This probe proves
// that each entry is still live: it GETs the entry's Homebrew Hub API record
// (the claim URL) and checks it names the same file, then reads the first
// 336 bytes of the resolved file with a Range request and checks the Game Boy
// header logo at 0x104. One timestamped line per step. Small GETs, no auth.
//
// Usage: plugins/homebrewhub/live-probe.ts [--record <dir>]
// --record rewrites the committed API record fixtures.
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { handlers } from "./plugin"

// First 16 bytes of the Nintendo logo at 0x104 in a Game Boy cartridge header.
const GB_LOGO = { offset: 0x104, hex: "ceed6666cc0d000b03730083000c000d" }
const providerId = "@korri:homebrewhub"

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

type Claim = { id: string; title: string; url: string; artifact: { system: string; file: { name: string } } }

// Every entry's search text names its system's legacy id.
const claims = handlers["claims.search"]({ query: "nintendo-gameboy" }) as Claim[]
check(claims.length > 0, `claims.search lists ${claims.length} entries`)

for (const claim of claims) {
  const record = await fetch(claim.url)
  const text = await record.text()
  log(`GET ${claim.url} -> ${record.status} ${text.length} chars ${record.headers.get("content-type") ?? ""}`)
  let files: string[] = []
  try {
    files = (JSON.parse(text).files ?? []).map((file: { filename: string }) => file.filename)
  } catch {}
  check(record.ok && files.includes(claim.artifact.file.name), `${claim.id}: API record lists ${claim.artifact.file.name} (${files.join(", ")})`)
  if (recordDir && record.ok) {
    writeFileSync(join(recordDir, `${claim.id}.json`), text.endsWith("\n") ? text : `${text}\n`)
    log(`recorded ${claim.id}.json`)
  }

  const resolution = handlers["artifact.resolve-download"]({ providerId, candidateUrl: claim.url }) as {
    _tag: string
    url?: string
    filename?: string
  }
  check(resolution._tag === "FinalDownload", `${claim.id}: artifact.resolve-download ${resolution._tag} ${resolution.url}`)
  if (resolution._tag !== "FinalDownload" || !resolution.url) continue

  const end = GB_LOGO.offset + 47
  const file = await fetch(resolution.url, { headers: { range: `bytes=0-${end}` } })
  const bytes = Buffer.from(await file.arrayBuffer())
  const total = file.headers.get("content-range")?.split("/")[1] ?? file.headers.get("content-length")
  log(`GET ${resolution.url} (bytes=0-${end}) -> ${file.status} total ${total} bytes ${file.headers.get("content-type") ?? ""}`)
  const logo = bytes.subarray(GB_LOGO.offset, GB_LOGO.offset + 16).toString("hex")
  check(file.ok && logo === GB_LOGO.hex, `${claim.id}: ${resolution.filename} carries the Game Boy header logo (${logo})`)
}

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
