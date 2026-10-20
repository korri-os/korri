import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import * as plugin from "./plugin"

// The RetroBrews repository READMEs that list each collection's games,
// recorded by live-probe.ts --record on 2026-10-20. No game bytes are committed.
const readme = (repository: string) =>
  readFileSync(new URL(`./fixtures/${repository}-README.md`, import.meta.url), "utf8")

// korrid's admission rules for `artifacts` and `games`, restated from
// services/korrid/src/plugin.rs (ADR 0013 implementation, worktree c-1fcdf054,
// commit 5ea5a70da): local keys are playable-id segments, artifact ids are
// `<plugin id>/<key>`, versions are one safe non-hidden segment, file hashes
// are `sha256:` + 64 hex, release hashes are sha256/sha1, a release with
// artifacts names its file inside one of them by a relative path.
const segment = /^[a-z0-9][a-z0-9._-]*$/
const sha256 = /^sha256:[0-9a-f]{64}$/
const safeRelativePath = (path: string) =>
  !path.startsWith("/") &&
  !/[\u0000-\u001f\u007f]/.test(path) &&
  path.split("/").every(part => part !== "" && part !== "..")
// Publisher systems.nix extensions for the systems this plugin names.
const systemExtensions: Record<string, string[]> = { gba: ["gba", "agb"], nes: ["nes"] }

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
    expect(games.ambushed.releases.c4efb1e252e3.system).toBe("nes")
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
        expect(release.hashes).toEqual([file.sha256])
        expect(artifact.version).toBe(file.sha256.slice("sha256:".length, "sha256:".length + 12))
        expect(releaseKey).toBe(artifact.version)
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
