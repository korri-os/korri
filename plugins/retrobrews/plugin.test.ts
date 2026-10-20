import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { handlers, providers } from "./plugin"

// The RetroBrews repository READMEs that list each collection's games,
// recorded by live-probe.ts --record on 2026-10-20. No game bytes are committed.
const readme = (repository: string) =>
  readFileSync(new URL(`./fixtures/${repository}-README.md`, import.meta.url), "utf8")

const providerId = "@korri:retrobrews"
const search = (query: string) => handlers["claims.search"]({ providerId, query, responses: [] } as never)
const details = (id: string) => handlers["claims.details"]({ providerId, id, responses: [] } as never)
const resolve = (candidateUrl: string) =>
  handlers["artifact.resolve-download"]({ providerId, candidateUrl, responses: [] } as never)

const ambushed = {
  _tag: "ProviderClaim",
  providerId,
  id: "nes-games:ambushed.nes",
  title: "Ambushed",
  url: "https://github.com/retrobrews/nes-games/blob/master/ambushed.nes",
  platform: "nes",
  artifact: {
    kind: "content",
    system: "nes",
    format: { id: "nes" },
    file: { name: "ambushed.nes", extension: "nes", mediaType: "application/octet-stream" },
  },
}
const anguna = {
  _tag: "ProviderClaim",
  providerId,
  id: "gba-games:anguna.gba",
  title: "Anguna",
  url: "https://github.com/retrobrews/gba-games/blob/master/anguna.gba",
  platform: "gba",
  artifact: {
    kind: "content",
    system: "gba",
    format: { id: "gba" },
    file: { name: "anguna.gba", extension: "gba", mediaType: "application/octet-stream" },
  },
}
const angunaFile = {
  _tag: "FinalDownload",
  providerId,
  url: "https://raw.githubusercontent.com/retrobrews/gba-games/master/anguna.gba",
  filename: "anguna.gba",
  contentType: "application/octet-stream",
}

describe("RetroBrews provider declaration", () => {
  test("declares one provider and the three acquisition handlers", () => {
    expect(providers).toStrictEqual({
      "@korri:retrobrews": { id: "@korri:retrobrews", title: "RetroBrews" },
    })
    expect(Object.keys(handlers).sort()).toEqual([
      "artifact.resolve-download",
      "claims.details",
      "claims.search",
    ])
  })

  test("each curated entry is listed with its author in its repository README", () => {
    expect(readme("nes-games")).toContain("<b>Ambushed</b> by SlyDog Studios")
    expect(details(ambushed.id).description).toBe("NES homebrew by SlyDog Studios.")
    expect(readme("gba-games")).toContain("<b>ANGUNA</b> by Nathan Tolbert")
    expect(details(anguna.id).description).toBe("GBA homebrew by Nathan Tolbert.")
  })
})

describe("claims.search", () => {
  test("matches the query against each entry's search text, without a request", () => {
    expect(search("nintendo-")).toStrictEqual([ambushed, anguna])
    expect(search(" Tolbert ")).toStrictEqual([anguna])
    expect(search("slydog")).toStrictEqual([ambushed])
  })

  test("answers a blank or unmatched query with no claims", () => {
    expect(search("")).toStrictEqual([])
    expect(search("2048")).toStrictEqual([])
  })
})

describe("claims.details", () => {
  test("describes the entry and names its file", () => {
    expect(details("gba-games:anguna.gba")).toStrictEqual({
      _tag: "ProviderClaimDetails",
      providerId,
      id: "gba-games:anguna.gba",
      title: "Anguna",
      url: anguna.url,
      description: "GBA homebrew by Nathan Tolbert.",
      downloadPageUrl: angunaFile.url,
      artifact: anguna.artifact,
    })
  })

  test("refuses an entry the catalog does not hold", () => {
    expect(() => details("gba-games:another-world.gba")).toThrow(
      "Unknown @korri:retrobrews candidate: gba-games:another-world.gba",
    )
  })
})

describe("artifact.resolve-download", () => {
  test("resolves the GitHub file page or the raw file to the raw file", () => {
    expect(resolve(anguna.url)).toStrictEqual(angunaFile)
    expect(resolve(angunaFile.url)).toStrictEqual(angunaFile)
    expect(resolve(ambushed.url)).toMatchObject({
      _tag: "FinalDownload",
      url: "https://raw.githubusercontent.com/retrobrews/nes-games/master/ambushed.nes",
      filename: "ambushed.nes",
    })
  })

  test("reports a RetroBrews file the catalog does not hold as not found", () => {
    expect(resolve("https://github.com/retrobrews/nes-games/blob/master/yun.nes")).toStrictEqual({
      _tag: "FailedDownload",
      providerId,
      reason: "not-found",
      message: "Unknown @korri:retrobrews candidate: nes-games:yun.nes",
    })
  })

  test("leaves other owners, branches, hosts and traversal unsupported", () => {
    for (const url of [
      "https://github.com/someone/gba-games/blob/master/anguna.gba",
      "https://github.com/retrobrews/gba-games/blob/main/anguna.gba",
      "https://example.com/retrobrews/gba-games/master/anguna.gba",
      "https://raw.githubusercontent.com/retrobrews/gba-games/master/../nes-games/master/ambushed.nes",
    ]) {
      expect(resolve(url)).toStrictEqual({ _tag: "NonFinalDownload", providerId, reason: "unsupported", url })
    }
  })
})
