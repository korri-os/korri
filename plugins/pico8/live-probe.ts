#!/usr/bin/env -S nix shell nixpkgs#bun --command bun
// Live probe for @korri:pico8. Not part of the unit suite.
//
// Runs the plugin's handlers against the live Lexaloffle BBS the way korrid
// runs a provider: call the handler, perform the GET requests it returns,
// call it again with the responses. Then it reads the first 64 bytes of the
// resolved cart file with a Range request and checks the PNG signature. It
// prints one timestamped line per step. Small GETs only, no auth.
//
// Usage: plugins/pico8/live-probe.ts [--record <dir>]
// --record rewrites the committed fixtures as excerpts of the live pages.
import { writeFileSync } from "node:fs"
import { join } from "node:path"
import type {
  DownloadResolution,
  PluginHttpRequest,
  PluginHttpRequests,
  PluginHttpResponse,
  ProviderClaim,
  ProviderClaimDetails,
} from "../../contracts/generated/korrid"
import { handlers } from "./plugin"

const QUERY = "celeste"
const SPECIMEN_POST = "86783"
const SPECIMEN_FILE = "celeste_classic_2-5.p8.png"
const OLD_CART_POST = "11722"
const UNKNOWN_POST = "999999999"
const EMPTY_QUERY = "zzqxjvkw"
const MAX_TURNS = 4
const PNG_SIGNATURE = "89504e470d0a1a0a"

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

function requestUrl(request: PluginHttpRequest): string {
  const query = Object.entries(request.query ?? {})
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`)
    .join("&")
  return query ? `${request.url}?${query}` : request.url
}

async function get(request: PluginHttpRequest): Promise<PluginHttpResponse> {
  const response = await fetch(requestUrl(request), { headers: request.headers ?? {} })
  const text = await response.text()
  const headers: Record<string, string> = {}
  response.headers.forEach((value, key) => (headers[key.toLowerCase()] = value))
  log(`GET ${requestUrl(request)} -> ${response.status} ${text.length} chars ${headers["content-type"] ?? ""}`)
  return { status: response.status, ok: response.ok, url: response.url, headers, text }
}

const isRequests = (output: unknown): output is PluginHttpRequests =>
  typeof output === "object" && output !== null && !Array.isArray(output) &&
  (output as { _tag?: unknown })._tag === "HttpRequests"

// korrid's replay loop: call, fetch what was asked, call again with every
// response so far, at most MAX_TURNS calls.
async function run<Result>(
  operation: string,
  call: (responses: PluginHttpResponse[]) => PluginHttpRequests | Result,
): Promise<{ result: Result; responses: PluginHttpResponse[] }> {
  const responses: PluginHttpResponse[] = []
  for (let turn = 1; turn <= MAX_TURNS; turn++) {
    const output = call([...responses])
    if (!isRequests(output)) return { result: output, responses }
    for (const request of output.requests) responses.push(await get(request))
  }
  throw new Error(`${operation} needed more than ${MAX_TURNS} turns`)
}

const providerId = "@korri:pico8"
const search = (query: string) =>
  run<ProviderClaim[]>("claims.search", responses => handlers["claims.search"]({ providerId, query, responses }))
const details = (id: string) =>
  run<ProviderClaimDetails>("claims.details", responses => handlers["claims.details"]({ providerId, id, responses }))
const resolve = (candidateUrl: string) =>
  run<DownloadResolution>("artifact.resolve-download", responses =>
    handlers["artifact.resolve-download"]({ providerId, candidateUrl, responses }),
  )

// Verbatim lines of a page, in page order: the head up to the viewport meta,
// the post container, every line that names a `.p8.png`, and the cart block
// from its title through its author.
function postExcerpt(html: string, id: string): string {
  const lines = html.split("\n")
  const headEnd = lines.findIndex(line => line.includes('name="viewport"'))
  const titleLine = lines.findIndex(line => line.includes("font-size:16pt; color:#fff"))
  const authorLine = titleLine < 0 ? -1 : lines.findIndex((line, index) => index > titleLine && line.includes("<b>"))
  return lines
    .filter(
      (line, index) =>
        index <= headEnd ||
        line.includes(`<div id=p${id} `) ||
        line.includes(".p8.png") ||
        (titleLine >= 0 && index >= titleLine && index <= authorLine),
    )
    .join("\n")
}

// The `pdat` and `updat` script arrays verbatim, or the BBS's line that
// says the search found nothing.
function searchExcerpt(html: string): string {
  const lines = html.split("\n")
  const start = lines.findIndex(line => line.includes("pdat=["))
  if (start < 0) return lines.filter(line => line.includes("[no posts found]")).join("\n")
  const updat = lines.findIndex(line => line.includes("var updat=["))
  return lines.slice(start, updat + 2).join("\n")
}

function record(name: string, text: string) {
  if (!recordDir) return
  writeFileSync(join(recordDir, name), `${text}\n`)
  log(`recorded ${name} (${text.length} chars)`)
}

const found = await search(QUERY)
const claims = found.result
check(claims.length > 0, `claims.search "${QUERY}" returned ${claims.length} claims`)
record("search-celeste.html", searchExcerpt(found.responses[0].text))
const specimen = claims.find(claim => claim.id === SPECIMEN_POST)
check(specimen !== undefined, `search lists post ${SPECIMEN_POST} ("${specimen?.title ?? "missing"}")`)

const empty = await search(EMPTY_QUERY)
check(empty.result.length === 0, `claims.search "${EMPTY_QUERY}" is empty`)
record("search-empty.html", searchExcerpt(empty.responses[0].text))

if (specimen) {
  const described = await details(specimen.id)
  const detail = described.result
  check(detail.downloadPageUrl?.endsWith(`/${SPECIMEN_FILE}`) === true, `claims.details ${specimen.id}: "${detail.title}" ${detail.downloadPageUrl}; ${detail.description}`)
  record(`post-${specimen.id}.html`, postExcerpt(described.responses[0].text, specimen.id))

  const { result: resolution } = await resolve(specimen.url)
  const final = resolution._tag === "FinalDownload" ? resolution : undefined
  check(final?.filename === SPECIMEN_FILE, `artifact.resolve-download ${specimen.url}: ${resolution._tag} ${final?.url ?? ""}`)

  if (final) {
    const cart = await fetch(final.url, { headers: { range: "bytes=0-63" } })
    const head = Buffer.from(await cart.arrayBuffer()).subarray(0, 8).toString("hex")
    const total = cart.headers.get("content-range")?.split("/")[1] ?? cart.headers.get("content-length")
    log(`GET ${final.url} (bytes=0-63) -> ${cart.status} total ${total} bytes ${cart.headers.get("content-type") ?? ""}`)
    check(head === PNG_SIGNATURE, `cart file starts with the PNG signature (${head})`)
  }
}

const old = await details(OLD_CART_POST)
check(/\/bbs\/cposts\/\d+\//.test(old.result.downloadPageUrl ?? ""), `claims.details ${OLD_CART_POST}: "${old.result.title}" ${old.result.downloadPageUrl}`)
record(`post-${OLD_CART_POST}.html`, postExcerpt(old.responses[0].text, OLD_CART_POST))

const unknown = await resolve(`https://www.lexaloffle.com/bbs/?pid=${UNKNOWN_POST}#p`)
const unknownResolution = unknown.result
check(unknownResolution._tag === "FailedDownload" && unknownResolution.reason === "not-found", `unknown post ${UNKNOWN_POST}: ${unknownResolution._tag}`)
record(`post-${UNKNOWN_POST}.html`, postExcerpt(unknown.responses[0].text, UNKNOWN_POST))

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
