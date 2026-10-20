import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { handlers, providers } from "./plugin"

// Homebrew Hub's API record for the entry, recorded by
// live-probe.ts --record on 2026-10-20. No game bytes are committed.
const apiRecord = JSON.parse(
  readFileSync(new URL("./fixtures/2048gb.json", import.meta.url), "utf8"),
) as { slug: string; title: string; developer: string; license: string; platform: string; basepath: string; files: Array<{ filename: string }> }

const providerId = "@korri:homebrewhub"
const search = (query: string) => handlers["claims.search"]({ providerId, query, responses: [] } as never)
const details = (id: string) => handlers["claims.details"]({ providerId, id, responses: [] } as never)
const resolve = (candidateUrl: string) =>
  handlers["artifact.resolve-download"]({ providerId, candidateUrl, responses: [] } as never)

const artifact = {
  kind: "content",
  system: "gb",
  format: { id: "gb" },
  file: { name: "2048.gb", extension: "gb", mediaType: "application/octet-stream" },
}
const claim = {
  _tag: "ProviderClaim",
  providerId,
  id: "2048gb",
  title: "2048gb",
  url: "https://hh3.gbdev.io/api/entry/2048gb.json",
  platform: "gb",
  artifact,
}

describe("Homebrew Hub provider declaration", () => {
  test("declares one provider and the three acquisition handlers", () => {
    expect(providers).toStrictEqual({
      "@korri:homebrewhub": { id: "@korri:homebrewhub", title: "Homebrew Hub" },
    })
    expect(Object.keys(handlers).sort()).toEqual([
      "artifact.resolve-download",
      "claims.details",
      "claims.search",
    ])
  })

  test("the curated entry agrees with Homebrew Hub's recorded API record", () => {
    const detail = details(apiRecord.slug)
    expect(detail.title).toBe(apiRecord.title)
    expect(detail.description).toContain(apiRecord.developer)
    expect(apiRecord.platform).toBe("GB")
    expect(apiRecord.license).toBe("Zlib")
    expect(apiRecord.files.map(file => file.filename)).toContain(detail.artifact?.file?.name)
    expect(detail.downloadPageUrl).toBe(
      `https://hh3.gbdev.io/static/${apiRecord.basepath}/entries/${apiRecord.slug}/${apiRecord.files[0].filename}`,
    )
  })
})

describe("claims.search", () => {
  test("matches the query against the entry's search text, without a request", () => {
    for (const query of ["2048", " SANQUI ", "zlib", "nintendo-gameboy"]) {
      expect(search(query)).toStrictEqual([claim])
    }
  })

  test("answers a blank or unmatched query with no claims", () => {
    expect(search("  ")).toStrictEqual([])
    expect(search("anguna")).toStrictEqual([])
  })
})

describe("claims.details", () => {
  test("describes the entry and names its file", () => {
    expect(details("2048gb")).toStrictEqual({
      _tag: "ProviderClaimDetails",
      providerId,
      id: "2048gb",
      title: "2048gb",
      url: "https://hh3.gbdev.io/api/entry/2048gb.json",
      description: "A Game Boy port of 2048 by Sanqui.",
      downloadPageUrl: "https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb",
      artifact,
    })
  })

  test("refuses an entry the catalog does not hold", () => {
    expect(() => details("basil-termini_2048-advance")).toThrow(
      "Unknown @korri:homebrewhub candidate: basil-termini_2048-advance",
    )
  })
})

describe("artifact.resolve-download", () => {
  const final = {
    _tag: "FinalDownload",
    providerId,
    url: "https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb",
    filename: "2048.gb",
    contentType: "application/octet-stream",
  }

  test("resolves the claim URL or a file URL of the entry to its file", () => {
    expect(resolve(claim.url)).toStrictEqual(final)
    expect(resolve(final.url)).toStrictEqual(final)
    expect(resolve("HTTPS://HH3.GBDEV.IO/api/entry/2048gb.json")).toStrictEqual(final)
  })

  test("reports an entry the catalog does not hold as not found", () => {
    expect(resolve("https://hh3.gbdev.io/api/entry/disabled-downloads.json")).toStrictEqual({
      _tag: "FailedDownload",
      providerId,
      reason: "not-found",
      message: "Unknown @korri:homebrewhub candidate: disabled-downloads",
    })
  })

  test("leaves other hosts and paths unsupported", () => {
    for (const url of [
      "https://example.com/api/entry/2048gb.json",
      "https://hh3.gbdev.io.example.com/api/entry/2048gb.json",
      "https://user@hh3.gbdev.io/api/entry/2048gb.json",
      "https://hh3.gbdev.io/entry/2048gb",
      "https://hh3.gbdev.io/api/entry/%E0%A4%A.json",
    ]) {
      expect(resolve(url)).toStrictEqual({ _tag: "NonFinalDownload", providerId, reason: "unsupported", url })
    }
  })
})
