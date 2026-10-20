/**
 * GAP — delete at integration, and import these names from
 * "@contracts/generated/korrid" instead.
 *
 * Hand-declared mirror of the six game-acquisition RPC methods the `build`
 * worker adds to services/korrid and Typeshare regenerates into
 * contracts/generated/korrid.ts. The narrowest shape the portal reads, taken
 * from the workflow contract (section 4) and the legacy names it preserves:
 * `ProviderRecord { id, title }` plus the owning plugin, legacy
 * `SearchRequest`/`SearchResponse`, `ProviderClaim`, `ProviderClaimDetails`,
 * `AcquireArtifactRequest` minus the fields korrid now takes from the claim,
 * and `AcquireJobStatus` with `state` as a tagged union. Casing follows the
 * generated file (camelCase on the wire; unions tagged on `_tag`).
 *
 * Unresolved until `build` reports the exact wire (each one is named in the
 * report): the failure codes below, and how "several eligible folders" lists
 * the folders to choose from (here: `sources` on the failure).
 */
import type { RpcFailure } from "@contracts/generated/korrid"

export interface AcquisitionProvider {
  id: string
  title: string
  /** The plugin that declares the provider. */
  pluginId: string
}

export interface AcquisitionProviders {
  providers: AcquisitionProvider[]
}

/** Legacy `ArtifactFile`, as much as the portal reads. */
export interface ClaimArtifactFile {
  name: string
  sizeBytes?: number
}

/** Legacy `ArtifactAcquisitionHint`, as much as the portal reads. */
export interface ClaimArtifact {
  /** The system id the registry knows. Absent: the claim cannot be acquired. */
  system?: string
  file?: ClaimArtifactFile
}

/** Legacy `ProviderClaim` minus `ref`/`playable` (contract section 3). */
export interface ProviderClaim {
  providerId: string
  id: string
  title: string
  url: string
  platform?: string
  thumbnailUrl?: string
  artifact?: ClaimArtifact
}

/** Legacy `ProviderClaimDetails` minus `ref`/`playable`/`facets`. */
export interface ProviderClaimDetails extends ProviderClaim {
  description?: string
}

export interface AcquisitionSearchRequest {
  query: string
  /** Exactly one in this run; more is a caller error. */
  providerIds: string[]
}

export interface AcquisitionSearchResponse {
  claims: ProviderClaim[]
}

export interface AcquisitionDetailsRequest {
  providerId: string
  id: string
}

export interface AcquisitionAcquireRequest {
  providerId: string
  id: string
  fileName?: string
  /** One of several eligible sources, after the person chose. */
  sourceId?: string
}

export interface AcquisitionJobRequest {
  jobId: string
}

export type AcquireJobState =
  | { _tag: "Resolving" }
  | { _tag: "Downloading"; bytesDone: number; bytesTotal?: number; bytesPerSecond?: number }
  | { _tag: "Placing" }
  | { _tag: "Imported"; gameId: string }
  | { _tag: "Staged"; message: string }
  | { _tag: "Failed"; reason: RpcFailure }
  | { _tag: "Cancelled" }

/** Legacy `AcquireJobStatus` with `stagedPath`/`placedPath` dropped. */
export interface AcquireJobStatus {
  jobId: string
  providerId: string
  id: string
  fileName?: string
  system?: string
  message?: string
  state: AcquireJobState
}

/** One folder a claim could be placed in. */
export interface AcquireSourceChoice {
  id: string
  label: string
}

/** RpcFailure, plus the folders to choose from when there are several. */
export interface AcquireFailure extends RpcFailure {
  sources?: AcquireSourceChoice[]
}

/**
 * The failure codes the portal tells apart. Every other code is shown with
 * korrid's own message.
 */
export const ACQUIRE_FAILURE = {
  /** No paired source holds the claim's system. Refused before download. */
  noSource: "AcquireNoSource",
  /** Several paired sources hold it; `sources` lists them. */
  severalSources: "AcquireSeveralSources",
  /** The claim names no system the registry knows. */
  unknownSystem: "AcquireUnknownSystem",
  /** The provider no longer offers the entry (legacy FailedDownload not-found). */
  unavailable: "AcquireEntryUnavailable",
  /** acquire-status / acquire-cancel for a job korrid does not hold. */
  jobNotFound: "NotFound",
} as const

export type AcquisitionProvidersOutcome =
  | { _tag: "Ok"; payload: AcquisitionProviders }
  | { _tag: "Err"; payload: RpcFailure }
export type AcquisitionSearchOutcome =
  | { _tag: "Ok"; payload: AcquisitionSearchResponse }
  | { _tag: "Err"; payload: RpcFailure }
export type AcquisitionDetailsOutcome =
  | { _tag: "Ok"; payload: ProviderClaimDetails }
  | { _tag: "Err"; payload: RpcFailure }
export type AcquireJobOutcome =
  | { _tag: "Ok"; payload: AcquireJobStatus }
  | { _tag: "Err"; payload: AcquireFailure }

export type StoreAcquisitionRpcRequest =
  | { _tag: "app.acquisition.providers"; payload: Record<string, never> }
  | { _tag: "app.acquisition.search"; payload: AcquisitionSearchRequest }
  | { _tag: "app.acquisition.details"; payload: AcquisitionDetailsRequest }
  | { _tag: "app.acquisition.acquire"; payload: AcquisitionAcquireRequest }
  | { _tag: "app.acquisition.acquire-status"; payload: AcquisitionJobRequest }
  | { _tag: "app.acquisition.acquire-cancel"; payload: AcquisitionJobRequest }
