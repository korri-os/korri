// Ported from the RetroBrews half of legacy
// product/plugins/acquisition-fixtures/index.ts (legacy
// 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62). A curated catalog: the handlers
// read only the entries below and never ask korrid for a request. korrid
// downloads the resolved file. The systems `gba` and `nes` and their runners
// are declared by the publisher's mgba and fceumm plugins; this file only
// references the ids.

export const name = "retrobrews"
export const title = "RetroBrews"
export const description =
  "Lists curated Game Boy Advance and NES homebrew from the RetroBrews collections."

const PROVIDER_ID = "@korri:retrobrews"

export const providers = {
  [PROVIDER_ID]: { id: PROVIDER_ID, title: "RetroBrews" },
}

// Local mirror of the legacy result shapes (legacy
// product/platform/protocol/acquisition/claim.ts, download-resolution.ts)
// minus `ref`, `playable`, `facets` and `requestHeaders`, as korrid admits
// them, and of the handler inputs in services/korrid/SCRIPTING.md, named as in
// contracts/generated/korrid.ts. Replace with the generated types once they
// land on main. This catalog never returns `HttpRequests`, so `responses` is
// always empty and unread.
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

interface PluginSearchInput {
  readonly providerId: string
  readonly query: string
  readonly responses: unknown[]
}

interface PluginDetailsInput {
  readonly providerId: string
  readonly id: string
  readonly responses: unknown[]
}

interface PluginResolveDownloadInput {
  readonly providerId: string
  readonly candidateUrl: string
  readonly fileName?: string
  readonly responses: unknown[]
}

// Legacy FixtureEntry. `system` replaces legacy `platform`, whose values were
// legacy system ids (`nintendo-nes`, `nintendo-gameboy-advance`); legacy
// pico8 already set `platform` to the system id. `disabledMessage` and
// `aliases` had no entry here. An id is `<repository>:<path>`.
interface CatalogEntry {
  readonly id: string
  readonly title: string
  readonly url: string
  readonly system: string
  readonly description: string
  readonly downloadUrl: string
  readonly filename: string
  readonly contentType: string
  readonly searchText: string
}

// Both legacy entries are live on 2026-10-20 (live-probe.ts).
const entries: CatalogEntry[] = [
  {
    id: "nes-games:ambushed.nes",
    title: "Ambushed",
    url: "https://github.com/retrobrews/nes-games/blob/master/ambushed.nes",
    system: "nes",
    description: "NES homebrew by SlyDog Studios.",
    downloadUrl: "https://raw.githubusercontent.com/retrobrews/nes-games/master/ambushed.nes",
    filename: "ambushed.nes",
    contentType: "application/octet-stream",
    searchText:
      "ambushed slydog studios homebrew rom arcade nintendo-nes nes-games ambushed.nes",
  },
  {
    id: "gba-games:anguna.gba",
    title: "Anguna",
    url: "https://github.com/retrobrews/gba-games/blob/master/anguna.gba",
    system: "gba",
    description: "GBA homebrew by Nathan Tolbert.",
    downloadUrl: "https://raw.githubusercontent.com/retrobrews/gba-games/master/anguna.gba",
    filename: "anguna.gba",
    contentType: "application/octet-stream",
    searchText: "anguna nathan tolbert nintendo-gameboy-advance gba-games anguna.gba",
  },
]

// --- handlers ---------------------------------------------------------------

function searchClaims(input: PluginSearchInput): ProviderClaim[] {
  const query = typeof input.query === "string" ? input.query.trim().toLowerCase() : ""
  if (query.length === 0) return []
  return entries
    .filter(entry => entry.searchText.toLowerCase().includes(query))
    .map(claimFor)
}

function claimDetails(input: PluginDetailsInput): ProviderClaimDetails {
  const id = stringField(input.id, "id")
  const entry = findEntry(id)
  if (!entry) throw new Error(unknownMessage(id))
  return detailsFor(entry)
}

function resolveDownload(input: PluginResolveDownloadInput): DownloadResolution {
  const candidateUrl = stringField(input.candidateUrl, "candidateUrl")
  const id = parseRetrobrewsUrl(candidateUrl)
  if (!id) {
    return {
      _tag: "NonFinalDownload",
      providerId: PROVIDER_ID,
      reason: "unsupported",
      url: candidateUrl,
    }
  }
  const entry = findEntry(id)
  if (!entry) {
    return {
      _tag: "FailedDownload",
      providerId: PROVIDER_ID,
      reason: "not-found",
      message: unknownMessage(id),
    }
  }
  return {
    _tag: "FinalDownload",
    providerId: PROVIDER_ID,
    url: entry.downloadUrl,
    filename: entry.filename,
    contentType: entry.contentType,
  }
}

export const handlers = {
  "claims.search": searchClaims,
  "claims.details": claimDetails,
  "artifact.resolve-download": resolveDownload,
}

// --- claims -----------------------------------------------------------------

function findEntry(id: string): CatalogEntry | undefined {
  return entries.find(entry => entry.id === id)
}

// Legacy names a downloaded file's format by its extension
// (artifact-acquisition.ts: `artifactFormat ?? artifactFileExtension(name)`).
function artifactFor(entry: CatalogEntry): ArtifactAcquisitionHint {
  const extension = fileExtension(entry.filename)
  return {
    kind: "content",
    system: entry.system,
    format: { id: extension ?? "binary" },
    file: {
      name: entry.filename,
      ...(extension ? { extension } : {}),
      mediaType: entry.contentType,
    },
  }
}

function claimFor(entry: CatalogEntry): ProviderClaim {
  return {
    _tag: "ProviderClaim",
    providerId: PROVIDER_ID,
    id: entry.id,
    title: entry.title,
    url: entry.url,
    platform: entry.system,
    artifact: artifactFor(entry),
  }
}

function detailsFor(entry: CatalogEntry): ProviderClaimDetails {
  return {
    _tag: "ProviderClaimDetails",
    providerId: PROVIDER_ID,
    id: entry.id,
    title: entry.title,
    url: entry.url,
    description: entry.description,
    downloadPageUrl: entry.downloadUrl,
    artifact: artifactFor(entry),
  }
}

function unknownMessage(id: string): string {
  return `Unknown ${PROVIDER_ID} candidate: ${id}`
}

// --- URLs (the plugin sandbox has no URL global) ----------------------------

// Legacy parseRetrobrewsUrl: a GitHub file page or its raw file, both on the
// `master` branch of a `retrobrews` repository.
function parseRetrobrewsUrl(input: string): string | null {
  const match = /^https?:\/\/([^/?#]+)(\/[^?#]*)?/i.exec(input.trim())
  if (!match) return null
  const host = match[1].toLowerCase()
  const pathname = match[2] ?? "/"
  const file =
    host === "github.com"
      ? /^\/retrobrews\/([^/]+)\/blob\/master\/(.+)$/.exec(pathname)
      : host === "raw.githubusercontent.com"
        ? /^\/retrobrews\/([^/]+)\/master\/(.+)$/.exec(pathname)
        : null
  if (!file || file[2].includes("..")) return null
  const repository = decodeComponent(file[1])
  const path = decodeComponent(file[2])
  return repository && path ? `${repository}:${path}` : null
}

function decodeComponent(value: string): string | null {
  try {
    return decodeURIComponent(value)
  } catch {
    return null
  }
}

// Legacy artifactFileExtension.
function fileExtension(fileName: string): string | undefined {
  const dot = fileName.lastIndexOf(".")
  if (dot <= 0 || dot === fileName.length - 1) return undefined
  const extension = fileName.slice(dot + 1).toLowerCase()
  return /^[a-z0-9]{1,10}$/.test(extension) ? extension : undefined
}

function stringField(value: unknown, key: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${key} is required`)
  }
  return value
}
