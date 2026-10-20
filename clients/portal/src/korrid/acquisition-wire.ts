/**
 * The ADR 0013 acquisition wire, ahead of its generated contract.
 *
 * The build branch adds `Queued`, `release` and the download/remove RPCs in
 * Rust and regenerates `contracts/generated/korrid.ts`. This branch does not
 * have that output yet, so the names it adds are written here, exactly as the
 * generated file will spell them. At merge, delete this file and import every
 * name below from `@contracts/generated/korrid` instead.
 */
import type {
  ArtifactState as GeneratedArtifactState,
  RpcFailure,
  RpcRequest as GeneratedRpcRequest,
  RpcResponse as GeneratedRpcResponse,
} from "@contracts/generated/korrid"

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export type ArtifactState = GeneratedArtifactState | { _tag: "Queued" }

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export interface ArtifactStatus {
	id: string;
	title: string;
	version: string;
	bytesTotal?: number;
	state: ArtifactState;
	release?: true;
}

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export interface AcquisitionSnapshot {
	artifacts: ArtifactStatus[];
}

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export type AcquisitionOutcome =
	| { _tag: "Ok", payload: AcquisitionSnapshot }
	| { _tag: "Err", payload: RpcFailure };

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export interface AcquisitionDownloadRequest {
	gameId: string;
}

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export interface AcquisitionRemoveRequest {
	gameId: string;
}

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export type RpcRequest =
	| GeneratedRpcRequest
	| { _tag: "app.acquisition.download", payload: AcquisitionDownloadRequest }
	| { _tag: "app.acquisition.remove", payload: AcquisitionRemoveRequest };

// Mirrors the build branch's generated contract; replace with @contracts/generated/korrid at merge.
export type RpcResponse =
	| GeneratedRpcResponse
	| { _tag: "app.acquisition.download", outcome: AcquisitionOutcome }
	| { _tag: "app.acquisition.remove", outcome: AcquisitionOutcome };
