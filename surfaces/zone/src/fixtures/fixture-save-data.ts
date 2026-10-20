/**
 * The SAVE DATA page's fixtures: identity slices typed against the treaty,
 * and a store that runs the real identity model (src/state/identity.ts)
 * with a stand-in for Korri behind it, so the page part answers every
 * press the way the product does.
 *
 * The page part may not import src/state (the atomic gate: shapes read no
 * state, and the gate counts parts as shapes). The store lives here, beside
 * the fixture host, and the part takes it as fixture data.
 *
 * Every value is fixed: no clock, no randomness. The keys and the secret are
 * shaped like the real ones (bech32 `npub1…` keys, a NIP-49 `ncryptsec1…`
 * backup of 162 characters) and generated from a fixed seed.
 */
import type { SurfaceIdentityManagement, SurfaceIdentityStatus } from "@contracts/surface/korri-surface"
import {
  IDENTITY_START,
  update,
  type IdentityMessage,
  type IdentityModel,
} from "../state/identity"
import type { ZoneRequest } from "../state/requests"

const BECH32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"

/** A fixed bech32-looking string: the same seed gives the same characters. */
function bech32Of(prefix: string, length: number, seed: number): string {
  let state = seed
  let out = prefix
  while (out.length < length) {
    state = (state * 1103515245 + 12345) % 2147483648
    out += BECH32[(state >>> 16) % BECH32.length]
  }
  return out
}

/** A NIP-49 encrypted backup: 162 characters, six lines on the slab. */
export const fixtureEncryptedSecret = bech32Of("ncryptsec1", 162, 7)
/** The key Korri switched to. */
export const fixtureOwnerKey = bech32Of("npub1", 63, 11)
/** Two retired keys. */
export const fixtureRetiredKeys: readonly string[] = [bech32Of("npub1", 63, 23), bech32Of("npub1", 63, 31)]
/** Fourteen retired keys: more than any screen shows at once. */
export const fixtureManyRetiredKeys: readonly string[] = Array.from({ length: 14 }, (_, index) => bech32Of("npub1", 63, 101 + index * 7))
/** A bunker URI of the length a real one has: a key and two relays and a secret. */
export const fixtureBunkerUri = `bunker://${bech32Of("", 64, 41).replace(/[^0-9a-f]/g, "a")}?relay=wss://relay.nsec.app&relay=wss://bunker.hummingbird-lake.ts.net&secret=${bech32Of("", 24, 43)}`

export const fixtureIdentity = (
  status: SurfaceIdentityStatus = { _tag: "Idle" },
  overrides: Partial<SurfaceIdentityManagement> = {},
): SurfaceIdentityManagement => ({
  localBackupAvailable: false,
  retiredPublicKeys: [],
  status,
  ...overrides,
})

export interface SaveDataScene {
  readonly model: IdentityModel
  readonly identity: SurfaceIdentityManagement | undefined
}

/** What the stand-in Korri does with a request: what the real one would publish next. */
function korriAnswers(identity: SurfaceIdentityManagement | undefined, request: ZoneRequest): SurfaceIdentityManagement | undefined {
  if (identity === undefined) return identity
  switch (request._tag) {
    case "ExportIdentityBackup":
      return { ...identity, localBackupAvailable: true, status: { _tag: "BackupReady", encryptedSecret: fixtureEncryptedSecret } }
    case "SwitchIdentityFromBackup":
    case "SwitchIdentityToNip46":
      return { ...identity, status: { _tag: "Switched", ownerPublicKey: fixtureOwnerKey } }
    case "DeleteRetiredIdentity":
      return { ...identity, retiredPublicKeys: identity.retiredPublicKeys.filter(key => key !== request.publicKey) }
    case "DismissIdentityStatus":
      return { ...identity, status: { _tag: "Idle" } }
    default:
      return identity
  }
}

/**
 * A store for one scene: the identity model and Korri's identity slice. A
 * message runs the real `update`; each request it returns is answered by
 * the stand-in Korri, which then publishes, as the real one does.
 */
export function createSaveDataStore(initial: SaveDataScene) {
  let scene = initial
  const listeners = new Set<() => void>()
  const requests: ZoneRequest[] = []
  const publish = () => {
    for (const listener of [...listeners]) listener()
  }
  return {
    requests: requests as readonly ZoneRequest[],
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    get: () => scene,
    dispatch(message: IdentityMessage) {
      const step = update(scene.model, message, scene.identity)
      let identity = scene.identity
      for (const request of step.requests) {
        requests.push(request)
        identity = korriAnswers(identity, request)
      }
      let model = step.model
      if (identity !== scene.identity) model = update(model, { _tag: "KorriPublished" }, identity).model
      if (model !== scene.model || identity !== scene.identity) {
        scene = { model, identity }
        publish()
      }
    },
  }
}

export type SaveDataStore = ReturnType<typeof createSaveDataStore>

/** A scene with the model at its start. */
export const sceneOf = (identity: SurfaceIdentityManagement | undefined, model: IdentityModel = IDENTITY_START): SaveDataScene =>
  ({ model, identity })
