import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import * as plugin from "./plugin"

// The RetroBrews repository READMEs that list each collection's games,
// recorded by live-probe.ts --record on 2026-10-20. No game bytes are committed.
const readme = (repository: string) =>
  readFileSync(new URL(`./fixtures/${repository}-README.md`, import.meta.url), "utf8")

// korrid's admission rules for `artifacts` and `games`, restated from
// services/korrid/src/plugin.rs (`normalize_plugin`): local keys are playable-id segments, artifact ids are
// `<plugin id>/<key>`, versions are one safe non-hidden segment, file hashes
// are `sha256:` + 64 hex, release hashes are sha256/sha1, a release with
// artifacts names its file inside one of them by a relative path.
const segment = /^[a-z0-9][a-z0-9._-]*$/
const sha256 = /^sha256:[0-9a-f]{64}$/
const safeRelativePath = (path: string) =>
  !path.startsWith("/") &&
  !/[\u0000-\u001f\u007f]/.test(path) &&
  path.split("/").every(part => part !== "" && part !== "..")
// Publisher systems.nix content rules for the systems this plugin names:
// extensions, and the header bytes discovery strips before hashing
// (`nes`: 16 bytes when they start `NES\x1a`; `gba` has none).
const systemExtensions: Record<string, string[]> = { gba: ["gba", "agb"], nes: ["nes"] }
const headerRules: Record<string, { size: number; hex: string }> = { nes: { size: 16, hex: "4e45531a" } }

const pluginId = "@korri:retrobrews"
const { artifacts, games } = plugin

describe("RetroBrews declaration", () => {
  test("declares games and pinned artifacts, and no provider or handlers", () => {
    expect(plugin.name).toBe("retrobrews")
    expect(Object.keys(plugin).sort()).toEqual(["artifacts", "description", "games", "name", "title"])
  })

  test("declares Ambushed (nes) and Anguna (gba) exactly, in legacy order", () => {
    expect(Object.keys(games)).toEqual(["ambushed", "anguna"])
    expect(artifacts.anguna).toStrictEqual({
      id: "@korri:retrobrews/anguna",
      version: "cd3b969d5ed6",
      title: "Anguna",
      files: [
        {
          url: "https://raw.githubusercontent.com/retrobrews/gba-games/master/anguna.gba",
          sha256: "sha256:cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851",
          bytes: 1710888,
        },
      ],
    })
    expect(games.anguna).toStrictEqual({
      title: "Anguna",
      releases: {
        cd3b969d5ed6: {
          system: "gba",
          hashes: ["sha256:cd3b969d5ed6b38b2039c11b894d6db2fc3fad209a7fd046bee241cc66069851"],
          artifacts: ["@korri:retrobrews/anguna"],
          file: { artifact: "@korri:retrobrews/anguna", path: "anguna.gba" },
        },
      },
    })
    expect(artifacts.ambushed.files).toStrictEqual([
      {
        url: "https://raw.githubusercontent.com/retrobrews/nes-games/master/ambushed.nes",
        sha256: "sha256:c4efb1e252e39dd957ecc76ff2a868159bb3a6dc66e07addfcdbf9e6660dfb62",
        bytes: 16400,
      },
    ])
    expect(games.ambushed).toStrictEqual({
      title: "Ambushed",
      releases: {
        // SHA-256 of bytes 16..end, after the iNES header.
        b9c6afa60056: {
          system: "nes",
          hashes: ["sha256:b9c6afa600568aa0144aed787acf778d6237187f89776b7d3548889700cd589a"],
          artifacts: ["@korri:retrobrews/ambushed"],
          file: { artifact: "@korri:retrobrews/ambushed", path: "ambushed.nes" },
        },
      },
    })
    expect(artifacts.ambushed.version).toBe("c4efb1e252e3")
  })

  test("every artifact passes korrid's artifact admission", () => {
    for (const [key, artifact] of Object.entries(artifacts)) {
      expect(key).toMatch(segment)
      expect(artifact.id).toBe(`${pluginId}/${key}`)
      expect(artifact.version).toMatch(/^[^./][^/\u0000-\u001f]*$/)
      expect(artifact.files.length).toBeGreaterThan(0)
      for (const file of artifact.files) {
        expect(file.sha256).toMatch(sha256)
        expect(file.url).toStartWith("https://")
        expect(file.bytes).toBeGreaterThan(0)
      }
      expect("unpack" in artifact).toBe(false)
    }
  })

  test("every release names its content file inside its own pinned artifact", () => {
    for (const [key, game] of Object.entries(games)) {
      expect(key).toMatch(segment)
      for (const [releaseKey, release] of Object.entries(game.releases)) {
        expect(releaseKey).toMatch(segment)
        expect(release.artifacts).toContain(release.file.artifact)
        expect(safeRelativePath(release.file.path)).toBe(true)
        const artifact = Object.values(artifacts).find(a => a.id === release.file.artifact)!
        expect(artifact).toBeDefined()
        // No unpack: the ready directory holds the file named by the URL's
        // percent-decoded last segment, and that file is the content file.
        const [file] = artifact.files
        expect(release.file.path).toBe(decodeURIComponent(file.url.split("/").at(-1)!))
        // The release hash is discovery's content hash: the whole file only
        // when the system strips no header (live-probe.ts measures both).
        expect(release.hashes).toHaveLength(1)
        expect(release.hashes[0]).toMatch(sha256)
        if (headerRules[release.system]) expect(release.hashes[0]).not.toBe(file.sha256)
        else expect(release.hashes[0]).toBe(file.sha256)
        // Release key from the content hash, artifact version from the file hash.
        expect(releaseKey).toBe(release.hashes[0].slice("sha256:".length, "sha256:".length + 12))
        expect(artifact.version).toBe(file.sha256.slice("sha256:".length, "sha256:".length + 12))
        // The system's runner accepts the file by its publisher content rules.
        expect(systemExtensions[release.system]).toContain(release.file.path.split(".").at(-1)!)
      }
    }
  })

  test("each game is listed with its author in its collection README", () => {
    expect(readme("nes-games")).toContain("<b>Ambushed</b> by SlyDog Studios")
    expect(readme("gba-games")).toContain("<b>ANGUNA</b> by Nathan Tolbert")
  })
})
