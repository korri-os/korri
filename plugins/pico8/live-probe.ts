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
import { handlers } from "./plugin"

const QUERY = "celeste"
const SPECIMEN_TITLE = "Celeste Classic 2"
const SPECIMEN_FILE = "celeste_classic_2-5.p8.png"
const OLD_CART_POST = "11722"
const UNKNOWN_POST = "999999999"
const EMPTY_QUERY = "zzqxjvkw"
const MAX_TURNS = 4
const PNG_SIGNATURE = "89504e470d0a1a0a"

type Request = {
  url: string
  query?: Record<string, string | number | boolean>
  headers?: Record<string, string>
}
type Response = {
  status: number
  ok: boolean
  url: string
  headers: Record<string, string>
  text: string
}

const log = (message: string) => console.log(`${new Date().toISOString()} ${message}`)
const recordDir = process.argv[2] === "--record" ? process.argv[3] : undefined
let failures = 0
const check = (ok: boolean, message: string) => {
  log(`${ok ? "PASS" : "FAIL"} ${message}`)
  if (!ok) failures += 1
}

function requestUrl(request: Request): string {
  const query = Object.entries(request.query ?? {})
    .map(([key, value]) => `${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`)
    .join("&")
  return query ? `${request.url}?${query}` : request.url
}

async function get(request: Request): Promise<Response> {
  const response = await fetch(requestUrl(request), { headers: request.headers ?? {} })
  const text = await response.text()
  const headers: Record<string, string> = {}
  response.headers.forEach((value, key) => (headers[key.toLowerCase()] = value))
  log(`GET ${requestUrl(request)} -> ${response.status} ${text.length} chars ${headers["content-type"] ?? ""}`)
  return { status: response.status, ok: response.ok, url: response.url, headers, text }
}

async function run(
  operation: keyof typeof handlers,
  input: Record<string, unknown>,
): Promise<{ result: unknown; responses: Response[] }> {
  const responses: Response[] = []
  const handler = handlers[operation] as (input: unknown) => unknown
  for (let turn = 1; turn <= MAX_TURNS; turn++) {
    const output = handler({ providerId: "@korri:pico8", ...input, responses }) as
      | { _tag?: string; requests?: Request[] }
      | undefined
    if (output && !Array.isArray(output) && output._tag === "HttpRequests") {
      for (const request of output.requests ?? []) responses.push(await get(request))
      continue
    }
    return { result: output, responses }
  }
  throw new Error(`${operation} needed more than ${MAX_TURNS} turns`)
}

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

const search = await run("claims.search", { query: QUERY })
const claims = search.result as Array<{ id: string; title: string; url: string }>
check(Array.isArray(claims) && claims.length > 0, `claims.search "${QUERY}" returned ${claims?.length ?? 0} claims`)
record("search-celeste.html", searchExcerpt(search.responses[0].text))
const specimen = claims.find(claim => claim.title === SPECIMEN_TITLE)
check(specimen !== undefined, `search lists "${SPECIMEN_TITLE}" (${specimen?.id ?? "missing"})`)

const empty = await run("claims.search", { query: EMPTY_QUERY })
check(Array.isArray(empty.result) && (empty.result as unknown[]).length === 0, `claims.search "${EMPTY_QUERY}" is empty`)
record("search-empty.html", searchExcerpt(empty.responses[0].text))

if (specimen) {
  const details = await run("claims.details", { id: specimen.id })
  const detail = details.result as { title: string; downloadPageUrl?: string; description?: string }
  check(detail.downloadPageUrl?.endsWith(`/${SPECIMEN_FILE}`) === true, `claims.details ${specimen.id}: "${detail.title}" ${detail.downloadPageUrl}; ${detail.description}`)
  record(`post-${specimen.id}.html`, postExcerpt(details.responses[0].text, specimen.id))

  const resolved = await run("artifact.resolve-download", { candidateUrl: specimen.url })
  const resolution = resolved.result as { _tag: string; url?: string; filename?: string }
  check(resolution._tag === "FinalDownload" && resolution.filename === SPECIMEN_FILE, `artifact.resolve-download ${specimen.url}: ${resolution._tag} ${resolution.url}`)

  if (resolution._tag === "FinalDownload" && resolution.url) {
    const cart = await fetch(resolution.url, { headers: { range: "bytes=0-63" } })
    const head = Buffer.from(await cart.arrayBuffer()).subarray(0, 8).toString("hex")
    const total = cart.headers.get("content-range")?.split("/")[1] ?? cart.headers.get("content-length")
    log(`GET ${resolution.url} (bytes=0-63) -> ${cart.status} total ${total} bytes ${cart.headers.get("content-type") ?? ""}`)
    check(head === PNG_SIGNATURE, `cart file starts with the PNG signature (${head})`)
  }
}

const old = await run("claims.details", { id: OLD_CART_POST })
const oldDetail = old.result as { title: string; downloadPageUrl?: string }
check(/\/bbs\/cposts\/\d+\//.test(oldDetail.downloadPageUrl ?? ""), `claims.details ${OLD_CART_POST}: "${oldDetail.title}" ${oldDetail.downloadPageUrl}`)
record(`post-${OLD_CART_POST}.html`, postExcerpt(old.responses[0].text, OLD_CART_POST))

const unknown = await run("artifact.resolve-download", { candidateUrl: `https://www.lexaloffle.com/bbs/?pid=${UNKNOWN_POST}#p` })
const unknownResolution = unknown.result as { _tag: string; reason?: string }
check(unknownResolution._tag === "FailedDownload" && unknownResolution.reason === "not-found", `unknown post ${UNKNOWN_POST}: ${unknownResolution._tag} ${unknownResolution.reason}`)
record(`post-${UNKNOWN_POST}.html`, postExcerpt(unknown.responses[0].text, UNKNOWN_POST))

log(failures === 0 ? "probe passed" : `probe failed: ${failures} check(s)`)
process.exit(failures === 0 ? 0 : 1)
