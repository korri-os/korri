/**
 * GAP — swap for the generated import at merge.
 *
 * Hand-declared mirror of the acquisition wire types the core worker adds to
 * services/korrid and Typeshare regenerates into contracts/generated/korrid.ts
 * (ADR 0012). Field names follow the generated file's casing (camelCase on
 * the wire); `ArtifactState` is internally tagged on `_tag`, like
 * `GameRouteSelection`. When the regenerated contract lands, delete this file
 * and import these names from "@contracts/generated/korrid"; nothing else in
 * the portal should need to change.
 */
import type { RpcFailure } from "@contracts/generated/korrid"

export type ArtifactState =
  | { _tag: "Missing" }
  | {
      _tag: "Downloading"
      /** 1-based index of the file being fetched, of `fileCount`. */
      file: number
      fileCount: number
      bytesDone: number
      bytesTotal?: number
      /** Rate sampled by korrid over the last 5 s. */
      bytesPerSecond?: number
    }
  | { _tag: "Unpacking" }
  | { _tag: "Ready"; path: string }
  | { _tag: "Failed"; reason: RpcFailure }

export interface ArtifactStatus {
  id: string
  /** Plugin title + artifact key unless the artifact declares a title. */
  title: string
  version: string
  bytesTotal?: number
  state: ArtifactState
}

export interface AcquisitionStatusRequest {
  gameId?: string
}

export interface AcquisitionStartRequest {
  artifactIds: string[]
}

export interface AcquisitionCancelRequest {
  artifactId: string
}

export interface AcquisitionArtifacts {
  artifacts: ArtifactStatus[]
}

export type AcquisitionOutcome =
  | { _tag: "Ok"; payload: AcquisitionArtifacts }
  | { _tag: "Err"; payload: RpcFailure }

export type AcquisitionRpcRequest =
  | { _tag: "app.acquisition.status"; payload: AcquisitionStatusRequest }
  | { _tag: "app.acquisition.start"; payload: AcquisitionStartRequest }
  | { _tag: "app.acquisition.cancel"; payload: AcquisitionCancelRequest }

export type AcquisitionRpcResponse = {
  _tag: AcquisitionRpcRequest["_tag"]
  outcome: AcquisitionOutcome
}
