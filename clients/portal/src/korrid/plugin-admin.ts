/**
 * The plugin admin shapes the portal reads that `contracts/generated/` does
 * not carry yet: publisher trust (ADR 0044) and requirements across
 * publishers (ADR 0049).
 *
 * They mirror `services/korrid/src/plugin_admin.rs` as the plugin-direction
 * contract states it ("Admin socket"; schema sheet row 17):
 * `PluginPublisher { namespace, fingerprint?, state, previousFingerprint?,
 * disclosure?, publicKey? }` on an inspection, on each dependency it brings,
 * and on a catalog entry in place of `publisherTrusted`; and the
 * `Publishers`, `TrustPublisher` and `RevokePublisher` requests.
 *
 * Every other field keeps the generated type, so these stay true when
 * `contracts/generated/korrid.ts` is regenerated with the same shapes. Once
 * it is, the portal should import them from there and this file goes.
 */
import type {
  PluginCatalogEntry as WirePluginCatalogEntry,
  PluginDependency as WirePluginDependency,
  PluginInspection as WirePluginInspection,
  RpcFailure,
} from "@contracts/generated/korrid"

/**
 * Where a publisher stands on this device.
 *
 * - `Bound`: the device's configuration names its key.
 * - `Trusted`: the owner trusted its key on this device (a pinned key).
 * - `New`: signed, by a key this device has not trusted.
 * - `KeyChanged`: signed by another key than the pinned one.
 * - `Unsigned`: a content-addressed release with no signature.
 *
 * A string union, not an enum, so a generated enum's values fit it.
 */
export type PluginPublisherState = "Bound" | "Trusted" | "New" | "KeyChanged" | "Unsigned"

export interface PluginPublisher {
  /** "@ada": the namespace the plugin id is published under. */
  readonly namespace: string
  /** The host's display form (sheet row 16): the key label, then the first
   * 16 characters of the base64 key in groups of four. Absent in a catalog,
   * and for an unsigned release. */
  readonly fingerprint?: string
  readonly state: PluginPublisherState
  /** `KeyChanged`: the pinned key's fingerprint. */
  readonly previousFingerprint?: string
  /** `Unsigned`: the disclosure the owner reads before approving (sheet row 14). */
  readonly disclosure?: string
  /** `New` and `KeyChanged`: the release's full public key (`label:base64`,
   * from the catalog record's `publicKey`, sheet row 7). TrustPublisher
   * sends exactly this. */
  readonly publicKey?: string
}

export type PluginCatalogEntry = Omit<WirePluginCatalogEntry, "publisherTrusted" | "publisher"> & {
  /** In a catalog only the state is certain: Bound or Trusted, else New. */
  readonly publisher: PluginPublisher
}

export interface PluginCatalog {
  /** Records for this host's platform only, in catalog order. */
  readonly entries: PluginCatalogEntry[]
}

export type PluginDependency = Omit<WirePluginDependency, "publisher"> & {
  readonly publisher: PluginPublisher
}

export type PluginInspection = Omit<WirePluginInspection, "brings" | "publisher"> & {
  readonly publisher: PluginPublisher
  readonly brings: PluginDependency[]
}

/** One publisher this device trusts, with the plugins installed from it. */
export interface PluginPublisherEntry {
  readonly namespace: string
  readonly fingerprint: string
  readonly state: Extract<PluginPublisherState, "Bound" | "Trusted">
  /** Installed plugin ids. */
  readonly installed: string[]
}

export interface PluginPublishers {
  readonly entries: PluginPublisherEntry[]
}

type Outcome<T> =
  | { readonly _tag: "Ok"; readonly payload: T }
  | { readonly _tag: "Err"; readonly payload: RpcFailure }

export type PluginCatalogOutcome = Outcome<PluginCatalog>
export type PluginInspectOutcome = Outcome<PluginInspection>
export type PluginPublishersOutcome = Outcome<PluginPublishers>

/** The wire tags of the three publisher requests (agreed with `host`). */
export const PUBLISHERS_TAG = "system.plugins.publishers"
export const TRUST_PUBLISHER_TAG = "system.plugins.publisher.trust"
export const REVOKE_PUBLISHER_TAG = "system.plugins.publisher.revoke"

/** Refusal codes the host adds for publisher trust and requirements. */
export const PublisherRefusal = {
  NotTrusted: "PluginPublisherNotTrusted",
  KeyChanged: "PluginPublisherKeyChanged",
  NamespaceTaken: "PluginNamespaceTaken",
  RequirementConflict: "PluginRequirementConflict",
} as const

/** The unsigned disclosure (sheet row 14). The host sends it as
 * `PluginPublisher.disclosure`; the portal shows the host's words, and this
 * text only when the host sends none. */
export const UNSIGNED_DISCLOSURE =
  "This plugin is not signed. Korri cannot confirm who made it, or that updates come from the same person. Install it only if you trust the address you got it from."
