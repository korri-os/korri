// Ported from the Homebrew Hub half of legacy
// product/plugins/acquisition-fixtures/index.ts (legacy
// 0e4cec9da3d77e6578b8a01a5d83420ba0d98e62). A curated catalog: the handlers
// read only the entries below and never ask korrid for a request. korrid
// downloads the resolved file. The system `gb` and its runner are declared by
// the publisher's gambatte plugin; this file only references the id.

export const name = "homebrewhub"
export const title = "Homebrew Hub"
export const description = "Lists curated Game Boy homebrew from Homebrew Hub."

const PROVIDER_ID = "@korri:homebrewhub"
const HOMEBREW_HUB_HOST = "hh3.gbdev.io"

export const providers = {
  [PROVIDER_ID]: { id: PROVIDER_ID, title: "Homebrew Hub" },
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

interface SearchInput {
  readonly query: string
}

interface DetailsInput {
  readonly id: string
}

interface ResolveDownloadInput {
  readonly candidateUrl: string
}

// Legacy FixtureEntry. `system` replaces legacy `platform`, whose values were
// legacy system ids (`nintendo-gameboy`); legacy pico8 already set `platform`
// to the system id. `disabledMessage` and `aliases` had no remaining entry.
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

// Live on 2026-10-20 (live-probe.ts). Legacy also listed
// `basil-termini_2048-advance` (GBA; its download URL answers 404) and the
// test-only `disabled-downloads` (no such entry upstream).
const entries: readonly CatalogEntry[] = [
  {
    id: "2048gb",
    title: "2048gb",
    url: "https://hh3.gbdev.io/api/entry/2048gb.json",
    system: "gb",
    description: "A Game Boy port of 2048 by Sanqui.",
    downloadUrl: "https://hh3.gbdev.io/static/database-gb/entries/2048gb/2048.gb",
    filename: "2048.gb",
    contentType: "application/octet-stream",
    searchText: "2048gb 2048 sanqui nintendo-gameboy zlib",
  },
]

// --- handlers ---------------------------------------------------------------

function searchClaims(input: SearchInput): readonly ProviderClaim[] {
  const query = typeof input.query === "string" ? input.query.trim().toLowerCase() : ""
  if (query.length === 0) return []
  return entries
    .filter(entry => entry.searchText.toLowerCase().includes(query))
    .map(claimFor)
}

function claimDetails(input: DetailsInput): ProviderClaimDetails {
  const id = stringField(input.id, "id")
  const entry = findEntry(id)
  if (!entry) throw new Error(unknownMessage(id))
  return detailsFor(entry)
}

function resolveDownload(input: ResolveDownloadInput): DownloadResolution {
  const candidateUrl = stringField(input.candidateUrl, "candidateUrl")
  const id = parseHomebrewHubUrl(candidateUrl)
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

// Legacy parseHomebrewHubUrl: an entry's API record or one of its files.
function parseHomebrewHubUrl(input: string): string | null {
  const match = /^https?:\/\/([^/?#]+)(\/[^?#]*)?/i.exec(input.trim())
  if (!match || match[1].toLowerCase() !== HOMEBREW_HUB_HOST) return null
  const pathname = match[2] ?? "/"
  const entry =
    /^\/api\/entry\/([^/]+)\.json$/.exec(pathname) ??
    /^\/static\/[^/]+\/entries\/([^/]+)\/.+$/.exec(pathname)
  return entry ? decodeComponent(entry[1]) : null
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
