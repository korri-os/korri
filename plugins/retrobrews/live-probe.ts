#!/usr/bin/env -S nix shell nixpkgs#bun --command bun
// Live probe for @korri:retrobrews. Not part of the unit suite.
//
// The catalog is curated, so the handlers make no requests. This probe proves
// that each entry is still live: it asks GitHub for the entry's file page
// (HEAD, the claim URL), reads the repository README that lists the games,
// then reads the first 64 bytes of the resolved file with a Range request and
// checks the system's header evidence from the publisher's systems.nix.
// One timestamped line per step. Small GETs, no auth.
//
// Usage: plugins/retrobrews/live-probe.ts [--record <dir>]
// --record rewrites the committed README fixtures.
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { handlers } from "./plugin"

// systems.nix `content.evidence.bytes` for each system this catalog names.
const HEADER_EVIDENCE: Record<string, { offset: number; hex: string }> = {
  nes: { offset: 0, hex: "4e45531a" },
  gba: { offset: 4, hex: "24ffae51699aa2213d84820a84e409ad" },
}
const providerId = "@korri:retrobrews"

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

type Claim = { id: string; title: string; url: string; artifact: { system: string; file: { name: string } } }

// Every entry's search text names its system's legacy `nintendo-` id.
const claims = handlers["claims.search"]({ providerId, query: "nintendo-", responses: [] }) as Claim[]
check(claims.length > 0, `claims.search lists ${claims.length} entries`)

for (const claim of claims) {
  const page = await fetch(claim.url, { method: "HEAD" })
  log(`HEAD ${claim.url} -> ${page.status}`)
  check(page.ok, `${claim.id}: GitHub file page answers`)

  const repository = claim.id.split(":")[0]
  const readmeUrl = `https://raw.githubusercontent.com/retrobrews/${repository}/master/README.md`
  const readme = await fetch(readmeUrl)
  const text = await readme.text()
  log(`GET ${readmeUrl} -> ${readme.status} ${text.length} chars`)
  check(readme.ok && text.toLowerCase().includes(`<b>${claim.title.toLowerCase()}</b>`), `${claim.id}: README lists "${claim.title}"`)
  if (recordDir && readme.ok) {
    writeFileSync(join(recordDir, `${repository}-README.md`), text)
    log(`recorded ${repository}-README.md`)
  }

  const resolution = handlers["artifact.resolve-download"]({ providerId, candidateUrl: claim.url, responses: [] }) as {
    _tag: string
    url?: string
    filename?: string
  }
  check(resolution._tag === "FinalDownload", `${claim.id}: artifact.resolve-download ${resolution._tag} ${resolution.url}`)
  if (resolution._tag !== "FinalDownload" || !resolution.url) continue

  const evidence = HEADER_EVIDENCE[claim.artifact.system]
  const file = await fetch(resolution.url, { headers: { range: "bytes=0-63" } })
  const bytes = Buffer.from(await file.arrayBuffer())
  const total = file.headers.get("content-range")?.split("/")[1] ?? file.headers.get("content-length")
  log(`GET ${resolution.url} (bytes=0-63) -> ${file.status} total ${total} bytes ${file.headers.get("content-type") ?? ""}`)
  const found = bytes.subarray(evidence.offset, evidence.offset + evidence.hex.length / 2).toString("hex")
  check(file.ok && found === evidence.hex, `${claim.id}: ${resolution.filename} carries the ${claim.artifact.system} header evidence (${found})`)
}

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
