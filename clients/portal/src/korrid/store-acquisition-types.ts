/**
 * GAP — delete at integration, and import these names from
 * "@contracts/generated/korrid" instead.
 *
 * Mirror of the game-acquisition wire the `build` worker exports through
 * Typeshare (build-3, f53e2cade, `services/korrid/SCRIPTING.md` "Acquisition
 * providers"), copied name for name. Placement into a person's folders is
 * withdrawn (workflow contract section 10): no `sourceId`, no ChooseSource;
 * a downloaded cart stays in korrid's cache. Every name below is
 * the generated one; only `StoreAcquisitionRpcRequest` is local, because the
 * generated RpcRequest union does not carry the six methods yet.
 */
import type { RpcFailure } from "@contracts/generated/korrid"

export type ArtifactKind = "content" | "patch"

export interface SemanticFormat {
  id: string
  version?: string
}

export interface ArtifactFile {
  name: string
  extension?: string
  mediaType?: string
  sizeBytes?: number
}

export interface ArtifactAcquisitionHint {
  kind: ArtifactKind
  /** The Korri system id. Absent: the claim is shown but cannot be acquired. */
  system?: string
  format: SemanticFormat
  file?: ArtifactFile
}

export interface ProviderClaim {
  _tag: "ProviderClaim"
  providerId: string
  id: string
  title: string
  url: string
  platform?: string
  thumbnailUrl?: string
  artifact?: ArtifactAcquisitionHint
  fetchedAt?: string
}

export interface ProviderClaimDetails {
  _tag: "ProviderClaimDetails"
  providerId: string
  id: string
  title: string
  url: string
  description?: string
  downloadPageUrl?: string
  artifact?: ArtifactAcquisitionHint
  fetchedAt?: string
}

export interface AcquisitionPluginsPayload {}

export interface AcquisitionProvider {
  id: string
  title: string
  pluginId: string
}

export interface PluginListResponse {
  providers: AcquisitionProvider[]
}

export interface SearchRequest {
  query: string
  /** Exactly one. */
  providerIds: string[]
}

export interface SearchResponse {
  claims: ProviderClaim[]
}

export interface DetailsRequest {
  providerId: string
  id: string
}

export interface AcquireArtifactRequest {
  providerId: string
  id: string
  fileName?: string
}

export interface AcquireStatusRequest {
  jobId: string
}

export interface AcquireCancelRequest {
  jobId: string
}

export type AcquireJobState =
  | { _tag: "Resolving" }
  | { _tag: "Downloading"; bytesDone: number; bytesTotal?: number; bytesPerSecond?: number }
  | { _tag: "Placing" }
  | { _tag: "Imported"; gameId: string }
  | { _tag: "Failed"; reason: RpcFailure }
  | { _tag: "Cancelled" }

export interface AcquireJobStatus {
  jobId: string
  providerId: string
  id: string
  fileName?: string
  system?: string
  message?: string
  state: AcquireJobState
}

export type PluginListOutcome =
  | { _tag: "Ok"; payload: PluginListResponse }
  | { _tag: "Err"; payload: RpcFailure }
export type SearchOutcome =
  | { _tag: "Ok"; payload: SearchResponse }
  | { _tag: "Err"; payload: RpcFailure }
export type DetailsOutcome =
  | { _tag: "Ok"; payload: ProviderClaimDetails }
  | { _tag: "Err"; payload: RpcFailure }
export type AcquireJobOutcome =
  | { _tag: "Ok"; payload: AcquireJobStatus }
  | { _tag: "Err"; payload: RpcFailure }

/** The codes the store tells apart (SCRIPTING.md, "Acquisition providers").
 * Every other code is shown with korrid's own message, which is plain copy. */
export const ACQUISITION_CODE = {
  /** `acquire-status`/`acquire-cancel` after a korrid restart. */
  unknownJob: "AcquisitionUnknownJob",
  /** The claim names no enabled system: it cannot be acquired. */
  systemUnknown: "AcquisitionSystemUnknown",
  /** Failed: the provider answered with a page, not the file (NonFinalDownload). */
  notFinal: "DownloadNotFinal",
  /** Failed: the provider said it cannot give the file (FailedDownload). */
  resolutionFailed: "DownloadResolutionFailed",
} as const

export type StoreAcquisitionRpcRequest =
  | { _tag: "app.acquisition.providers"; payload: AcquisitionPluginsPayload }
  | { _tag: "app.acquisition.search"; payload: SearchRequest }
  | { _tag: "app.acquisition.details"; payload: DetailsRequest }
  | { _tag: "app.acquisition.acquire"; payload: AcquireArtifactRequest }
  | { _tag: "app.acquisition.acquire-status"; payload: AcquireStatusRequest }
  | { _tag: "app.acquisition.acquire-cancel"; payload: AcquireCancelRequest }
