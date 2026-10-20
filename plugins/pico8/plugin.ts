// Ported from legacy product/plugins/pico8/src/plugin.ts (legacy
// 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62). The legacy plugin fetched the
// Lexaloffle BBS itself. Here every handler is a pure request/response turn:
// it returns the GET requests it needs, korrid performs them, and korrid calls
// the handler again with `responses` (services/korrid/SCRIPTING.md,
// "Acquisition providers"). The system `pico8` and its runner are declared by
// the publisher's fake08 plugin; this file only references the id.

export const name = "pico8"
export const title = "PICO-8 BBS"
export const description =
  "Searches the Lexaloffle BBS for PICO-8 cartridges and resolves their cart files."

const PROVIDER_ID = "@korri:pico8"
const PICO8_SYSTEM = "pico8"
const PICO8_CART_CONTENT_TYPE = "image/png"
const BBS_HOST = "www.lexaloffle.com"
const BBS_BASE_URL = `https://${BBS_HOST}`
const MAX_SEARCH_RESULTS = 50

export const providers = {
  [PROVIDER_ID]: { id: PROVIDER_ID, title: "PICO-8 BBS" },
}

// Legacy result shapes (legacy product/platform/protocol/acquisition/claim.ts,
// download-resolution.ts) minus `ref`, `playable`, `facets` and
// `requestHeaders`.
interface ArtifactAcquisitionHint {
  readonly kind: "content" | "patch"
  readonly system?: string
  readonly format: { readonly id: string; readonly version?: string }
  readonly file?: {
    readonly name: string
    readonly extension?: string
    readonly mediaType?: string
    readonly sizeBytes?: number
  }
}

interface ProviderClaim {
  readonly _tag: "ProviderClaim"
  readonly providerId: string
  readonly id: string
  readonly title: string
  readonly url: string
  readonly platform?: string
  readonly thumbnailUrl?: string
  readonly artifact?: ArtifactAcquisitionHint
  readonly fetchedAt?: string
}

interface ProviderClaimDetails {
  readonly _tag: "ProviderClaimDetails"
  readonly providerId: string
  readonly id: string
  readonly title: string
  readonly url: string
  readonly description?: string
  readonly downloadPageUrl?: string
  readonly artifact?: ArtifactAcquisitionHint
  readonly fetchedAt?: string
}

interface DownloadChoice {
  readonly id: string
  readonly fileName?: string
  readonly size?: string
  readonly platforms?: readonly string[]
}

type DownloadResolution =
  | {
      readonly _tag: "FinalDownload"
      readonly providerId: string
      readonly url: string
      readonly filename?: string
      readonly contentType?: string
    }
  | {
      readonly _tag: "NonFinalDownload"
      readonly providerId: string
      readonly reason: "interstitial" | "requires-user-action" | "unsupported"
      readonly url?: string
      readonly choices?: readonly DownloadChoice[]
    }
  | {
      readonly _tag: "FailedDownload"
      readonly providerId: string
      readonly reason:
        | "provider-error"
        | "configuration"
        | "not-found"
        | "defective-provider"
      readonly message: string
    }

// Turn protocol. Field names follow legacy PluginHttpRequestOptions and
// PluginHttpResponse. The tag is the build worker's draft of SCRIPTING.md and
// is not final yet; it lives only in this constant.
const HTTP_REQUESTS_TAG = "HttpRequests"

interface PluginHttpRequest {
  readonly url: string
  readonly query?: Readonly<Record<string, string | number | boolean>>
  readonly headers?: Readonly<Record<string, string>>
  readonly timeoutMs?: number
}

interface PluginHttpResponse {
  readonly status: number
  readonly ok: boolean
  readonly url: string
  readonly headers: Readonly<Record<string, string>>
  readonly text: string
}

interface HttpRequests {
  readonly _tag: typeof HTTP_REQUESTS_TAG
  readonly requests: readonly PluginHttpRequest[]
}

type Turn<Result> = HttpRequests | Result

interface TurnInput {
  readonly providerId?: string
  readonly responses?: readonly PluginHttpResponse[]
}

interface SearchInput extends TurnInput {
  readonly query: string
}

interface DetailsInput extends TurnInput {
  readonly id: string
}

interface ResolveDownloadInput extends TurnInput {
  readonly candidateUrl: string
  readonly fileName?: string
}

const HTML_HEADERS = { accept: "text/html,application/xhtml+xml" } as const

function httpRequests(...requests: PluginHttpRequest[]): HttpRequests {
  return { _tag: HTTP_REQUESTS_TAG, requests }
}

// Legacy searchUrl(): the BBS cart listing filtered by `search`.
function searchRequest(query: string): PluginHttpRequest {
  return {
    url: `${BBS_BASE_URL}/bbs/`,
    query: {
      cat: "7",
      sub: "2",
      mode: "carts",
      orderby: "ts",
      search: query,
      max: String(MAX_SEARCH_RESULTS),
    },
    headers: HTML_HEADERS,
  }
}

function postRequest(id: string): PluginHttpRequest {
  return { url: `${BBS_BASE_URL}/bbs/`, query: { pid: id }, headers: HTML_HEADERS }
}

function postUrl(id: string): string {
  return `${BBS_BASE_URL}/bbs/?pid=${encodeURIComponent(id)}#p`
}

// --- handlers: thin adapters over the pure parsers below -----------------

function searchClaims(input: SearchInput): Turn<readonly ProviderClaim[]> {
  const query = typeof input.query === "string" ? input.query.trim() : ""
  if (query.length === 0) return []
  const page = firstResponse(input)
  if (!page) return httpRequests(searchRequest(query))
  if (!page.ok) throw new Error(`PICO-8 BBS search returned HTTP ${page.status}`)
  const entries = parsePico8SearchEntries(page.text)
  // Legacy read a page without the cart list as no results. That hides a
  // markup change, so only the BBS's own empty answer counts as empty.
  if (!entries) throw new Error("PICO-8 BBS search page has no cart list")
  return entries.slice(0, MAX_SEARCH_RESULTS).map(claimFor)
}

function claimDetails(input: DetailsInput): Turn<ProviderClaimDetails> {
  const id = stringField(input.id, "id")
  const page = firstResponse(input)
  if (!page) return httpRequests(postRequest(id))
  if (!page.ok) throw new Error(`PICO-8 BBS post returned HTTP ${page.status}`)
  const details = parsePico8Details(page.text, id)
  if (!details) throw new Error(`Unknown PICO-8 BBS candidate: ${id}`)
  return detailsFor(details)
}

function resolveDownload(input: ResolveDownloadInput): Turn<DownloadResolution> {
  const candidateUrl = stringField(input.candidateUrl, "candidateUrl")
  const parsed = parsePico8BbsUrl(candidateUrl)
  if (!parsed) return nonFinal("unsupported", candidateUrl)
  if (parsed.downloadUrl) return finalDownload(parsed.downloadUrl)
  if (!parsed.id) return nonFinal("unsupported", candidateUrl)
  const page = firstResponse(input)
  if (!page) return httpRequests(postRequest(parsed.id))
  if (page.status === 404) {
    return failed("not-found", `Unknown PICO-8 BBS candidate: ${parsed.id}`)
  }
  if (!page.ok) {
    return failed("provider-error", `PICO-8 BBS post returned HTTP ${page.status}`)
  }
  const details = parsePico8Details(page.text, parsed.id)
  if (!details) return failed("not-found", `Unknown PICO-8 BBS candidate: ${parsed.id}`)
  if (!details.downloadUrl) return nonFinal("requires-user-action", details.url)
  return finalDownload(details.downloadUrl)
}

export const handlers = {
  "claims.search": searchClaims,
  "claims.details": claimDetails,
  "artifact.resolve-download": resolveDownload,
}

// --- claims ---------------------------------------------------------------

interface Pico8BbsEntry {
  readonly id: string
  readonly title: string
  readonly url: string
  readonly author?: string
  readonly thumbnailUrl?: string
  readonly createdAt?: string
  readonly updatedAt?: string
}

interface Pico8CartDetails extends Pico8BbsEntry {
  readonly downloadUrl?: string
  readonly filename?: string
}

// Legacy names a downloaded file's format by its last extension
// (artifact-acquisition.ts: `artifactFormat ?? artifactFileExtension(name)`),
// so a `.p8.png` cart is format `png`. A search row does not name the cart
// file yet; details and resolve-download do.
function artifactHint(filename?: string): ArtifactAcquisitionHint {
  return {
    kind: "content",
    system: PICO8_SYSTEM,
    format: { id: "png" },
    ...(filename
      ? {
          file: {
            name: filename,
            extension: "png",
            mediaType: PICO8_CART_CONTENT_TYPE,
          },
        }
      : {}),
  }
}

function claimFor(entry: Pico8BbsEntry): ProviderClaim {
  return {
    _tag: "ProviderClaim",
    providerId: PROVIDER_ID,
    id: entry.id,
    title: entry.title,
    url: entry.url,
    platform: PICO8_SYSTEM,
    ...(entry.thumbnailUrl ? { thumbnailUrl: entry.thumbnailUrl } : {}),
    artifact: artifactHint(),
  }
}

function detailsFor(entry: Pico8CartDetails): ProviderClaimDetails {
  return {
    _tag: "ProviderClaimDetails",
    providerId: PROVIDER_ID,
    id: entry.id,
    title: entry.title,
    url: entry.url,
    description: descriptionFor(entry),
    ...(entry.downloadUrl ? { downloadPageUrl: entry.downloadUrl } : {}),
    artifact: artifactHint(entry.filename),
  }
}

function descriptionFor(entry: Pico8CartDetails): string {
  return [
    entry.author ? `PICO-8 BBS cart by ${entry.author}.` : "PICO-8 BBS cart.",
    entry.createdAt ? `Published ${entry.createdAt}.` : undefined,
    entry.updatedAt ? `Updated ${entry.updatedAt}.` : undefined,
  ]
    .filter((part): part is string => part !== undefined)
    .join(" ")
}

function finalDownload(url: string): DownloadResolution {
  return {
    _tag: "FinalDownload",
    providerId: PROVIDER_ID,
    url,
    filename: filenameFromUrl(url) ?? "pico8-cart.p8.png",
    contentType: PICO8_CART_CONTENT_TYPE,
  }
}

function nonFinal(
  reason: "interstitial" | "requires-user-action" | "unsupported",
  url: string,
): DownloadResolution {
  return { _tag: "NonFinalDownload", providerId: PROVIDER_ID, reason, url }
}

function failed(
  reason: "provider-error" | "not-found",
  message: string,
): DownloadResolution {
  return { _tag: "FailedDownload", providerId: PROVIDER_ID, reason, message }
}

// --- BBS search page: the `pdat=[...]` JS array ---------------------------

// Row fields used: 0 post id, 2 thread title, 3 thumbnail path, 6 created,
// 8 author, 9 updated, 22 cart title. Legacy prefers the cart title.
function parsePico8SearchEntries(html: string): readonly Pico8BbsEntry[] | null {
  const rows = pdatRows(html)
  if (!rows) return html.includes("[no posts found]") ? [] : null
  return rows.flatMap(row => {
    const fields = splitTopLevel(row)
    const id = stringLiteral(fields[0])
    if (!id) return []
    const title = stringLiteral(fields[22]) ?? stringLiteral(fields[2])
    if (!title) return []
    return [
      {
        id,
        title: decodeHtml(title),
        url: postUrl(id),
        author: stringLiteral(fields[8]),
        thumbnailUrl: absoluteBbsUrl(stringLiteral(fields[3])),
        createdAt: stringLiteral(fields[6]),
        updatedAt: stringLiteral(fields[9]),
      },
    ]
  })
}

function pdatRows(html: string): readonly string[] | null {
  const assignment = html.indexOf("pdat=[")
  if (assignment < 0) return null
  const start = html.indexOf("[", assignment)

  const rows: string[] = []
  let depth = 0
  let rowStart = -1
  let quote: "'" | '"' | "`" | null = null
  let escaped = false

  for (let index = start; index < html.length; index++) {
    const char = html[index]

    if (quote) {
      if (escaped) {
        escaped = false
      } else if (char === "\\") {
        escaped = true
      } else if (char === quote) {
        quote = null
      }
      continue
    }

    if (char === "'" || char === '"' || char === "`") {
      quote = char
      continue
    }

    if (char === "[") {
      depth += 1
      if (depth === 2) rowStart = index + 1
      continue
    }

    if (char === "]") {
      if (depth === 2 && rowStart >= 0) {
        rows.push(html.slice(rowStart, index))
        rowStart = -1
      }
      depth -= 1
      if (depth === 0) break
    }
  }

  return rows
}

function splitTopLevel(row: string): readonly string[] {
  const fields: string[] = []
  let start = 0
  let depth = 0
  let quote: "'" | '"' | "`" | null = null
  let escaped = false

  for (let index = 0; index < row.length; index++) {
    const char = row[index]

    if (quote) {
      if (escaped) {
        escaped = false
      } else if (char === "\\") {
        escaped = true
      } else if (char === quote) {
        quote = null
      }
      continue
    }

    if (char === "'" || char === '"' || char === "`") {
      quote = char
      continue
    }

    if (char === "[") {
      depth += 1
      continue
    }
    if (char === "]") {
      depth -= 1
      continue
    }
    if (char === "," && depth === 0) {
      fields.push(row.slice(start, index).trim())
      start = index + 1
    }
  }

  fields.push(row.slice(start).trim())
  return fields
}

function stringLiteral(value: string | undefined): string | undefined {
  const trimmed = value?.trim()
  if (!trimmed) return undefined
  const quote = trimmed[0]
  if (
    (quote === "'" || quote === '"' || quote === "`") &&
    trimmed.endsWith(quote)
  ) {
    return trimmed.slice(1, -1).replaceAll(`\\${quote}`, quote)
  }
  return trimmed === "" ? undefined : trimmed
}

// --- BBS post page --------------------------------------------------------

function parsePico8Details(html: string, id: string): Pico8CartDetails | null {
  const cartFilePath = firstMatch(html, /href=["']([^"']+\.p8\.png)["']/i)
  const downloadUrl = absoluteBbsUrl(cartFilePath)
  // The BBS answers an unknown post id with its front page and status 200.
  // Legacy then invented "PICO-8 Cart <id>". A real post page carries the
  // post container `<div id=p<id>`.
  if (!downloadUrl && !new RegExp(`<div id=["']?p${escapeRegExp(id)}\\b`).test(html)) {
    return null
  }
  const title =
    titleNearCartFile(html) ??
    metaContent(html, "og:title") ??
    `PICO-8 Cart ${id}`
  const author = authorNearCartFile(html)
  const filename = downloadUrl ? filenameFromUrl(downloadUrl) : undefined
  return {
    id,
    title: decodeHtml(title),
    url: postUrl(id),
    ...(author ? { author } : {}),
    ...(downloadUrl ? { downloadUrl } : {}),
    ...(filename ? { filename } : {}),
  }
}

function titleNearCartFile(html: string): string | undefined {
  const cartFileIndex = html.search(/Cart File/i)
  if (cartFileIndex < 0) return undefined
  const before = html.slice(Math.max(0, cartFileIndex - 1000), cartFileIndex)
  const matches = [...before.matchAll(/<div[^>]*>([^<]+)<\/div>/gi)]
  return cleanText(matches.at(-1)?.[1])
}

function authorNearCartFile(html: string): string | undefined {
  const cartFileIndex = html.search(/Cart File/i)
  if (cartFileIndex < 0) return undefined
  const around = html.slice(cartFileIndex, cartFileIndex + 1500)
  return cleanText(firstMatch(around, /<b>([^<]+)<\/b>/i))
}

function metaContent(html: string, property: string): string | undefined {
  return cleanText(
    firstMatch(
      html,
      new RegExp(
        `<meta\\s+property=["']${escapeRegExp(property)}["'][^>]*content=["']([^"']+)["']`,
        "i",
      ),
    ),
  )
}

// --- BBS URLs (the plugin sandbox has no URL global) ----------------------

interface ParsedPico8Url {
  readonly id?: string
  readonly downloadUrl?: string
}

interface BbsLocation {
  readonly pathname: string
  readonly params: Readonly<Record<string, string>>
}

// Accepts absolute BBS URLs and site-relative paths. Any other host,
// embedded credentials or scheme is null.
function parseBbsLocation(input: string): BbsLocation | null {
  const trimmed = input.trim()
  let rest: string
  const absolute = /^https?:\/\/([^/?#]*)([^#]*)/i.exec(trimmed)
  if (absolute) {
    if (absolute[1].toLowerCase() !== BBS_HOST) return null
    rest = absolute[2] || "/"
  } else if (trimmed.startsWith("/") && !trimmed.startsWith("//")) {
    rest = trimmed.split("#")[0]
  } else {
    return null
  }
  const queryIndex = rest.indexOf("?")
  const pathname = queryIndex < 0 ? rest : rest.slice(0, queryIndex)
  const params: Record<string, string> = {}
  if (queryIndex >= 0) {
    for (const pair of rest.slice(queryIndex + 1).split("&")) {
      if (!pair) continue
      const eq = pair.indexOf("=")
      const key = decodeComponent(eq < 0 ? pair : pair.slice(0, eq))
      const value = decodeComponent(eq < 0 ? "" : pair.slice(eq + 1))
      if (key !== undefined && value !== undefined && !(key in params)) {
        params[key] = value
      }
    }
  }
  return { pathname, params }
}

function parsePico8BbsUrl(input: string): ParsedPico8Url | null {
  const location = parseBbsLocation(input)
  if (!location) return null
  // Old carts live under a numeric folder (`/bbs/cposts/1/15133.p8.png`),
  // current ones under the first two characters of the cart id
  // (`/bbs/cposts/ce/celeste_classic_2-5.p8.png`, the BBS player's
  // `lid.substr(0,2)`). Legacy accepted digits only.
  const cartFile = /^\/bbs\/cposts\/[a-z0-9_-]+\/[A-Za-z0-9._-]+\.p8\.png$/i.test(
    location.pathname,
  )
  if (cartFile) return { downloadUrl: `${BBS_BASE_URL}${location.pathname}` }

  if (location.pathname !== "/bbs/") return null
  const id = location.params.pid ?? location.params.tid
  return id ? { id } : null
}

function absoluteBbsUrl(path: string | undefined): string | undefined {
  if (!path) return undefined
  if (/^https?:\/\//i.test(path)) {
    return parseBbsLocation(path) ? path : undefined
  }
  if (!path.startsWith("/") || path.startsWith("//")) return undefined
  return `${BBS_BASE_URL}${path}`
}

function filenameFromUrl(input: string): string | undefined {
  const withoutQuery = input.split("#")[0].split("?")[0]
  const segments = withoutQuery.split("/").filter(Boolean)
  const last = segments.at(-1)
  return last && segments.length > 2 ? last : undefined
}

function decodeComponent(value: string): string | undefined {
  try {
    return decodeURIComponent(value.replaceAll("+", " "))
  } catch {
    return undefined
  }
}

// --- text -----------------------------------------------------------------

function firstMatch(input: string, pattern: RegExp): string | undefined {
  return input.match(pattern)?.[1]
}

function cleanText(value: string | undefined): string | undefined {
  const cleaned = value ? decodeHtml(stripTags(value)).trim() : ""
  return cleaned.length > 0 ? cleaned : undefined
}

function stripTags(value: string): string {
  return value.replaceAll(/<[^>]*>/g, "")
}

function decodeHtml(value: string): string {
  const named: Record<string, string> = {
    amp: "&",
    gt: ">",
    lt: "<",
    quot: '"',
    apos: "'",
    nbsp: " ",
  }
  return value
    .replaceAll(/&#(\d+);/g, (_, code: string) =>
      String.fromCodePoint(Number(code)),
    )
    .replaceAll(/&#x([0-9a-f]+);/gi, (_, code: string) =>
      String.fromCodePoint(Number.parseInt(code, 16)),
    )
    .replaceAll(/&([a-z]+);/gi, (match, name: string) => named[name] ?? match)
}

function escapeRegExp(value: string): string {
  return value.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&")
}

function firstResponse(input: TurnInput): PluginHttpResponse | undefined {
  const responses = input.responses
  return Array.isArray(responses) && responses.length > 0 ? responses[0] : undefined
}

function stringField(value: unknown, key: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${key} is required`)
  }
  return value
}
