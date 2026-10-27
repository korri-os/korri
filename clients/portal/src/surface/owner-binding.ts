import type { OwnerBindingSnapshot } from "@contracts/generated/korrid"
import type { SurfaceStatus } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

export const OWNER_BINDING_ACTION = "owner-binding"
export const OWNER_BINDING_CONTINUE = "owner-binding-continue"

export type OwnerBindingState =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Unsupported" }
  | { readonly _tag: "Ready"; readonly snapshot: OwnerBindingSnapshot }
  | { readonly _tag: "Submitting"; readonly snapshot: OwnerBindingSnapshot }
  | { readonly _tag: "Problem"; readonly message: string }

/** Uses the existing confirmation treaty. A surface never sees an Android API,
 * a signer URI, a private key, or a second backend. */
export function ownerBindingStatus(state: OwnerBindingState, setupNeeded: boolean): SurfaceStatus | undefined {
  if (state._tag === "Loading" || state._tag === "Unsupported") return undefined
  if (state._tag === "Problem") return {
    _tag: "Confirm", kicker: "Could not read identity", detail: state.message,
    actions: [{ id: OWNER_BINDING_ACTION, label: "Check identity", enabled: true }],
  }
  const snapshot = state.snapshot
  if (snapshot.identity._tag === "Owned") return setupNeeded ? {
    _tag: "Confirm", kicker: "Identity connected",
    detail: "Your signer keeps your person key. Korri can now find your other devices.",
    actions: [{ id: OWNER_BINDING_CONTINUE, label: "Open library", enabled: true }],
  } : undefined
  if (snapshot.identity._tag !== "Unowned") return undefined
  if (state._tag === "Submitting" || snapshot.personSigner._tag === "Pending") return {
    _tag: "Busy", kicker: "Approve in your signer",
    detail: "Choose the same person key as your other Korri devices and approve this device's owner binding.",
  }
  const problem = ["Denied", "InvalidResponse", "Defect"].includes(snapshot.personSigner._tag)
    ? snapshot.personSigner.message : undefined
  return {
    _tag: "Confirm", kicker: "Connect your identity",
    detail: [problem, snapshot.signerRequirement,
      "Use the same person key as your other Korri devices. The private key stays in your signer."].filter(Boolean).join(" "),
    actions: [{ id: OWNER_BINDING_ACTION,
      label: snapshot.signerRequirement !== undefined ? "Check for signer" : "Set up owner", enabled: true }],
  }
}

/** Observes the native effect boundary and the verified peer list. Only a
 * membership/state change reloads the library; relay heartbeat times do not. */
export function createOwnerBindingController(
  korrid: KorridClient,
  context: { readonly changed: () => void; readonly finish: () => void },
) {
  let state: OwnerBindingState = { _tag: "Loading" }
  const listeners = new Set<() => void>()
  let active = false
  let generation = 0
  let read = 0
  let submitting = false
  let timer: ReturnType<typeof setTimeout> | undefined
  let peerSignature: string | undefined
  const publish = (next: OwnerBindingState) => {
    state = next
    for (const listener of listeners) listener()
  }
  const previous = () => state._tag === "Ready" || state._tag === "Submitting" ? state.snapshot : undefined
  const observe = async () => {
    if (!active || submitting) return
    const epoch = generation
    const request = ++read
    const outcome = await korrid.ownerBindingSnapshot()
    if (!active || epoch !== generation || request !== read) return
    if (outcome._tag === "Err") {
      if (outcome.payload.code === "OperationUnsupported") publish({ _tag: "Unsupported" })
      else publish({ _tag: "Problem", message: outcome.payload.message })
      return
    }
    publish({ _tag: "Ready", snapshot: outcome.payload })
    if (outcome.payload.identity._tag !== "Owned") return
    const peers = await korrid.peerList()
    if (!active || epoch !== generation || request !== read || peers._tag !== "Ok") return
    const signature = JSON.stringify(peers.payload.peers.map(peer => [peer.devicePublicKey, peer.label, peer.state])
      .sort((a, b) => a[0]!.localeCompare(b[0]!)))
    if (signature !== peerSignature) {
      peerSignature = signature
      context.changed()
    }
  }
  const poll = async (epoch: number) => {
    await observe()
    if (active && generation === epoch && state._tag !== "Unsupported") {
      timer = setTimeout(() => void poll(epoch), 1500)
    }
  }
  return {
    snapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener) } },
    start() {
      submitting = false
      active = true
      const epoch = ++generation
      void poll(epoch)
      return () => { active = false; ++generation; ++read; clearTimeout(timer) }
    },
    refresh: observe,
    act(id: string): boolean {
      if (id === OWNER_BINDING_CONTINUE) {
        if (previous()?.identity._tag === "Owned") context.finish()
        return true
      }
      if (id !== OWNER_BINDING_ACTION) return false
      const snapshot = previous()
      if (!active || submitting) return true
      if (snapshot?.personSigner._tag === "Pending") { void observe(); return true }
      if (snapshot === undefined || snapshot.signerRequirement !== undefined) { void observe(); return true }
      if (snapshot.identity._tag !== "Unowned") return true
      submitting = true
      ++read
      const epoch = generation
      publish({ _tag: "Submitting", snapshot })
      void korrid.startOwnerBinding().then(outcome => {
        if (!active || epoch !== generation) return
        submitting = false
        if (outcome._tag === "Ok") publish({ _tag: "Ready", snapshot: outcome.payload })
        else publish({ _tag: "Problem", message: outcome.payload.message })
      })
      return true
    },
  }
}
