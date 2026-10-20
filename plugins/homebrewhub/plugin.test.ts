import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import * as plugin from "./plugin"

// Homebrew Hub's API record for the entry, recorded by
// live-probe.ts --record on 2026-10-20. No game bytes are committed.
const apiRecord = JSON.parse(
  readFileSync(new URL("./fixtures/2048gb.json", import.meta.url), "utf8"),
) as { slug: string; title: string; developer: string; license: string; platform: string; basepath: string; files: Array<{ filename: string }> }

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
// extensions, and the header bytes discovery strips before hashing (none).
const systemExtensions: Record<string, string[]> = { gb: ["gb", "dmg", "sgb"] }
const headerRules: Record<string, { size: number; hex: string }> = {}

const pluginId = "@korri:homebrewhub"
const { artifacts, games } = plugin

describe("Homebrew Hub declaration", () => {
  test("declares games and pinned artifacts, and no provider or handlers", () => {
    expect(plugin.name).toBe("homebrewhub")
    expect(Object.keys(plugin).sort()).toEqual(["artifacts", "description", "games", "name", "title"])
  })

  test("declares 2048gb exactly", () => {
    expect(artifacts).toStrictEqual({
      "2048gb": {
        id: "@korri:homebrewhub/2048gb",
        version: "3ea2376b15b3",
        title: "2048gb",
        files: [
          {
            url: "https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb",
            sha256: "sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f",
            bytes: 32768,
          },
        ],
      },
    })
    expect(games).toStrictEqual({
      "2048gb": {
        title: "2048gb",
        releases: {
          "3ea2376b15b3": {
            system: "gb",
            hashes: ["sha256:3ea2376b15b34bd26b10e6b31d2753bf8b086098dbdb5ee84d9c0f03d66d3d7f"],
            artifacts: ["@korri:homebrewhub/2048gb"],
            file: { artifact: "@korri:homebrewhub/2048gb", path: "2048.gb" },
          },
        },
      },
    })
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

  test("the entry agrees with Homebrew Hub's recorded API record", () => {
    const game = games["2048gb"]
    const release = game.releases["3ea2376b15b3"]
    expect(game.title).toBe(apiRecord.title)
    expect(apiRecord.platform).toBe("GB")
    expect(apiRecord.license).toBe("Zlib")
    expect(apiRecord.files.map(file => file.filename)).toContain(release.file.path)
    expect(artifacts["2048gb"].files[0].url).toBe(
      `https://hh3.gbdev.io/static/${apiRecord.basepath}/entries/${apiRecord.slug}/${release.file.path}`,
    )
  })
})
