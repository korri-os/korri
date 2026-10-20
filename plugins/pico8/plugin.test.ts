import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"
import { handlers, providers } from "./plugin"

// Excerpts of live Lexaloffle BBS pages, recorded by live-probe.ts --record
// on 2026-10-20. No cart bytes are committed.
const fixture = (name: string) =>
  readFileSync(new URL(`./fixtures/${name}`, import.meta.url), "utf8")
const searchPage = fixture("search-celeste.html")
const emptySearchPage = fixture("search-empty.html")
const currentPost = fixture("post-86783.html")
const oldPost = fixture("post-11722.html")
const unknownPost = fixture("post-999999999.html")

const providerId = "@korri:pico8"
const ok = (text: string, url = "https://www.lexaloffle.com/bbs/") => ({
  status: 200,
  ok: true,
  url,
  headers: { "content-type": "text/html; charset=UTF-8" },
  text,
})
const status = (code: number) => ({ ...ok(""), status: code, ok: false })

const search = (query: string, responses: unknown[] = []) =>
  handlers["claims.search"]({ providerId, query, responses } as never)
const details = (id: string, responses: unknown[] = []) =>
  handlers["claims.details"]({ providerId, id, responses } as never)
const resolve = (candidateUrl: string, responses: unknown[] = []) =>
  handlers["artifact.resolve-download"]({ providerId, candidateUrl, responses } as never)

const postRequest = (pid: string) => ({
  _tag: "HttpRequests",
  requests: [
    {
      url: "https://www.lexaloffle.com/bbs/",
      query: { pid },
      headers: { accept: "text/html,application/xhtml+xml" },
    },
  ],
})

describe("PICO-8 BBS provider declaration", () => {
  test("declares one provider and the three acquisition handlers", () => {
    expect(providers).toStrictEqual({
      "@korri:pico8": { id: "@korri:pico8", title: "PICO-8 BBS" },
    })
    expect(Object.keys(handlers).sort()).toEqual([
      "artifact.resolve-download",
      "claims.details",
      "claims.search",
    ])
  })
})

describe("claims.search", () => {
  test("asks for the BBS cart listing filtered by the query", () => {
    expect(search("  celeste ")).toStrictEqual({
      _tag: "HttpRequests",
      requests: [
        {
          url: "https://www.lexaloffle.com/bbs/",
          query: {
            cat: "7",
            sub: "2",
            mode: "carts",
            orderby: "ts",
            search: "celeste",
            max: "50",
          },
          headers: { accept: "text/html,application/xhtml+xml" },
        },
      ],
    })
  })

  test("answers a blank query with no claims and no request", () => {
    expect(search("   ")).toStrictEqual([])
  })

  test("builds one claim per cart row of the recorded listing", () => {
    const claims = search("celeste", [ok(searchPage)]) as Array<{ id: string; title: string }>
    expect(claims).toHaveLength(30)
    expect(claims.slice(0, 2)).toStrictEqual([
      {
        _tag: "ProviderClaim",
        providerId,
        id: "11722",
        title: "Celeste 1.0 (Fixed for P8 v0.1.2)",
        url: "https://www.lexaloffle.com/bbs/?pid=11722#p",
        platform: "pico8",
        thumbnailUrl: "https://www.lexaloffle.com/bbs/thumbs/pico15133.png",
        artifact: { kind: "content", system: "pico8", format: { id: "png" } },
      },
      {
        _tag: "ProviderClaim",
        providerId,
        id: "86783",
        title: "Celeste Classic 2",
        url: "https://www.lexaloffle.com/bbs/?pid=86783#p",
        platform: "pico8",
        thumbnailUrl: "https://www.lexaloffle.com/bbs/thumbs/pico8_celeste_classic_2-5.png",
        artifact: { kind: "content", system: "pico8", format: { id: "png" } },
      },
    ])
    // Rows with tag arrays and non-ASCII titles keep their field positions.
    expect(claims[3]).toMatchObject({ id: "105950", title: "violet" })
  })

  test("reads the BBS's own empty answer as no results", () => {
    expect(search("zzqxjvkw", [ok(emptySearchPage)])).toStrictEqual([])
  })

  test("fails when the BBS errors or the page has no cart list", () => {
    expect(() => search("celeste", [status(503)])).toThrow("HTTP 503")
    expect(() => search("celeste", [ok(unknownPost)])).toThrow("no cart list")
  })
})

describe("claims.details", () => {
  test("asks for the post page", () => {
    expect(details("86783")).toStrictEqual(postRequest("86783"))
  })

  test("reads title, author and the cart file under its two-character folder", () => {
    expect(details("86783", [ok(currentPost)])).toStrictEqual({
      _tag: "ProviderClaimDetails",
      providerId,
      id: "86783",
      title: "Celeste Classic 2",
      url: "https://www.lexaloffle.com/bbs/?pid=86783#p",
      description: "PICO-8 BBS cart by noel.",
      downloadPageUrl: "https://www.lexaloffle.com/bbs/cposts/ce/celeste_classic_2-5.p8.png",
      artifact: {
        kind: "content",
        system: "pico8",
        format: { id: "png" },
        file: { name: "celeste_classic_2-5.p8.png", extension: "png", mediaType: "image/png" },
      },
    })
  })

  test("still reads an old cart under its numeric folder", () => {
    expect(details("11722", [ok(oldPost)])).toMatchObject({
      title: "Celeste 1.0 (Fixed for P8 v0.1.2)",
      downloadPageUrl: "https://www.lexaloffle.com/bbs/cposts/1/15133.p8.png",
      artifact: { file: { name: "15133.p8.png" } },
    })
  })

  test("refuses an unknown post, which the BBS answers with its front page", () => {
    expect(() => details("999999999", [ok(unknownPost)])).toThrow(
      "Unknown PICO-8 BBS candidate: 999999999",
    )
    expect(() => details("86783", [status(500)])).toThrow("HTTP 500")
    expect(() => details(" ")).toThrow("id is required")
  })
})

describe("artifact.resolve-download", () => {
  const claimUrl = "https://www.lexaloffle.com/bbs/?pid=86783#p"
  const celesteCart = {
    _tag: "FinalDownload",
    providerId,
    url: "https://www.lexaloffle.com/bbs/cposts/ce/celeste_classic_2-5.p8.png",
    filename: "celeste_classic_2-5.p8.png",
    contentType: "image/png",
  }

  test("resolves a claim URL through its post page to the cart file", () => {
    expect(resolve(claimUrl)).toStrictEqual(postRequest("86783"))
    expect(resolve(claimUrl, [ok(currentPost)])).toStrictEqual(celesteCart)
  })

  test("accepts cart file URLs directly, in both folder layouts, without a request", () => {
    expect(resolve(celesteCart.url)).toStrictEqual(celesteCart)
    expect(resolve("https://www.lexaloffle.com/bbs/cposts/1/15133.p8.png")).toMatchObject({
      _tag: "FinalDownload",
      filename: "15133.p8.png",
    })
  })

  test("leaves other hosts and paths unsupported", () => {
    for (const url of [
      "https://example.com/bbs/?pid=86783",
      "https://www.lexaloffle.com/pico-8.php",
      "https://www.lexaloffle.com.example.com/bbs/?pid=1",
    ]) {
      expect(resolve(url)).toStrictEqual({
        _tag: "NonFinalDownload",
        providerId,
        reason: "unsupported",
        url,
      })
    }
  })

  test("reports a post without a cart file as needing the person", () => {
    const withoutCart = currentPost
      .split("\n")
      .filter(line => !line.includes(".p8.png"))
      .join("\n")
    expect(resolve(claimUrl, [ok(withoutCart)])).toStrictEqual({
      _tag: "NonFinalDownload",
      providerId,
      reason: "requires-user-action",
      url: claimUrl,
    })
  })

  test("reports unknown posts and BBS errors as failed resolutions", () => {
    const unknownUrl = "https://www.lexaloffle.com/bbs/?pid=999999999#p"
    expect(resolve(unknownUrl, [ok(unknownPost)])).toStrictEqual({
      _tag: "FailedDownload",
      providerId,
      reason: "not-found",
      message: "Unknown PICO-8 BBS candidate: 999999999",
    })
    expect(resolve(claimUrl, [status(404)])).toMatchObject({ reason: "not-found" })
    expect(resolve(claimUrl, [status(502)])).toMatchObject({
      _tag: "FailedDownload",
      reason: "provider-error",
    })
  })
})
