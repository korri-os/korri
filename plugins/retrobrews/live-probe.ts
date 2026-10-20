#!/usr/bin/env -S nix shell nixpkgs#bun --command bun
// Live probe for @korri:retrobrews. Not part of the unit suite.
//
// Proves each pinned artifact is still what plugin.ts says: it GETs every
// artifact file (the largest is 1.7 MB) and compares status, size and SHA-256
// with the pin, checks the system's header evidence from the publisher's
// systems.nix, and checks that the collection README still lists the game.
// One timestamped line per step. GETs only, no auth.
//
// Usage: plugins/retrobrews/live-probe.ts [--record <dir>]
// --record rewrites the committed README fixtures.
import { createHash } from "node:crypto"
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import { artifacts, games } from "./plugin"

// systems.nix `content.evidence.bytes` for each system this plugin names.
const HEADER_EVIDENCE: Record<string, { offset: number; hex: string }> = {
  nes: { offset: 0, hex: "4e45531a" },
  gba: { offset: 4, hex: "24ffae51699aa2213d84820a84e409ad" },
}

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

const recorded = new Set<string>()
for (const [key, game] of Object.entries(games)) {
  for (const release of Object.values(game.releases)) {
    const artifact = Object.values(artifacts).find(a => a.id === release.file.artifact)!
    for (const pin of artifact.files) {
      const response = await fetch(pin.url, { redirect: "manual" })
      const bytes = Buffer.from(await response.arrayBuffer())
      const sha256 = `sha256:${createHash("sha256").update(bytes).digest("hex")}`
      log(`GET ${pin.url} -> ${response.status} ${bytes.length} bytes ${sha256}`)
      check(response.status === 200, `${artifact.id}: answers 200 without a redirect`)
      check(bytes.length === pin.bytes && sha256 === pin.sha256, `${artifact.id}: size and SHA-256 match the pin`)
      const evidence = HEADER_EVIDENCE[release.system]
      const found = bytes.subarray(evidence.offset, evidence.offset + evidence.hex.length / 2).toString("hex")
      check(found === evidence.hex, `${artifact.id}: carries the ${release.system} header evidence (${found})`)
    }

    const repository = artifact.files[0].url.split("/")[4]
    const readmeUrl = `https://raw.githubusercontent.com/retrobrews/${repository}/master/README.md`
    const readme = await fetch(readmeUrl)
    const text = await readme.text()
    log(`GET ${readmeUrl} -> ${readme.status} ${text.length} chars`)
    check(readme.ok && text.toLowerCase().includes(`<b>${game.title.toLowerCase()}</b>`), `${key}: README lists "${game.title}"`)
    if (recordDir && readme.ok && !recorded.has(repository)) {
      writeFileSync(join(recordDir, `${repository}-README.md`), text)
      recorded.add(repository)
      log(`recorded ${repository}-README.md`)
    }
  }
}

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
