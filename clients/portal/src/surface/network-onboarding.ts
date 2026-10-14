import type { NetworkSnapshot, WifiAttempt } from "@contracts/generated/korrid"
import type { SurfaceNetwork, SurfaceWifiAttempt } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

function attemptFrom(attempt: WifiAttempt): SurfaceWifiAttempt {
  switch (attempt._tag) {
    case "Idle": return { _tag: "Idle" }
    case "Connecting": return { _tag: "Connecting", ...attempt.payload }
    case "Failed": return { _tag: "Failed", ...attempt.payload }
  }
}
export function networkFrom(snapshot: NetworkSnapshot): SurfaceNetwork {
  return {
    connection: snapshot.connection._tag === "Wifi"
      ? { _tag: "Wifi", ...snapshot.connection.payload }
      : { _tag: snapshot.connection._tag },
    wifi: snapshot.wifi._tag === "NoAdapter" ? { _tag: "NoAdapter" }
      : { _tag: "Available", ...snapshot.wifi.payload, attempt: attemptFrom(snapshot.wifi.payload.attempt) },
  }
}
export interface NetworkOnboardingState {
  readonly network?: SurfaceNetwork
  readonly onboardingNeeded?: boolean
  readonly problem?: string
}
const FAILURE = "Could not change the network or save setup progress. Try again."

/** One host owner for reads and commands. Passwords exist only in call arguments. */
export function createNetworkOnboarding(korrid: KorridClient) {
  let state: NetworkOnboardingState = {}
  const listeners = new Set<() => void>()
  let generation = 0
  let active = true
  let busy = false
  let reading = false
  let readGeneration = 0
  let onboardingRevision = 0
  let finishGeneration = 0
  let finishing = false
  const publish = (next: NetworkOnboardingState) => {
    if (!active || JSON.stringify(next) === JSON.stringify(state)) return
    state = next
    for (const listener of [...listeners]) listener()
  }
  const refresh = async () => {
    if (!active || busy || reading) return
    reading = true
    const read = ++readGeneration
    const current = ++generation
    const onboardingReadRevision = onboardingRevision
    try {
      const [network, onboarding] = await Promise.all([korrid.networkStatus(), korrid.onboardingStatus()])
      if (!active || current !== generation) return
      publish({ ...state,
        network: network._tag === "Ok" ? networkFrom(network.payload) : undefined,
        ...(onboarding._tag === "Ok" && onboardingReadRevision === onboardingRevision ? { onboardingNeeded: onboarding.payload.needed } : {}),
        ...(onboarding._tag === "Err" && onboardingReadRevision === onboardingRevision ? { problem: "Could not read setup progress. Try again." } : {}),
      })
    } catch { if (active && current === generation) publish({ ...state, problem: FAILURE }) }
    finally { if (read === readGeneration) reading = false }
  }
  const connect = async (target: string, hidden: boolean, password?: string) => {
    if (!active || busy) return
    busy = true
    const current = ++generation
    const wifi = state.network?.wifi
    const ssid = hidden ? target : wifi?._tag === "Available" ? wifi.networks.find(item => item.id === target)?.ssid : undefined
    publish({ ...state, problem: undefined,
      ...(wifi?._tag === "Available" && ssid !== undefined && state.network ? { network: { ...state.network, wifi: { ...wifi, attempt: { _tag: "Connecting", ssid } } } } : {}),
    })
    try {
      const outcome = hidden ? await korrid.connectHiddenWifi(target, password) : await korrid.connectWifi(target, password)
      if (!active || current !== generation) return
      if (outcome._tag === "Ok") publish({ ...state, network: networkFrom(outcome.payload) })
      else {
        const network = state.network
        publish({ ...state, problem: "Could not join the network. Scan and try again.",
          ...(network?.wifi._tag === "Available" && ssid !== undefined ? { network: { ...network, wifi: { ...network.wifi, attempt: { _tag: "Failed", ssid, message: "Could not join the network. Try again.", passwordRejected: false } } } } : {}),
        })
      }
    } catch { if (active && current === generation) {
      const network = state.network
      publish({ ...state, problem: FAILURE, ...(network?.wifi._tag === "Available" && ssid !== undefined ? { network: { ...network, wifi: { ...network.wifi, attempt: { _tag: "Failed", ssid, message: "Could not join the network. Try again.", passwordRejected: false } } } } : {}) })
    } } finally { if (current === generation) busy = false }
  }
  return {
    snapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener) } },
    refresh,
    start() {
      active = true
      void refresh()
      const interval = setInterval(() => void refresh(), 2_000)
      return () => { active = false; busy = false; reading = false; finishing = false; ++finishGeneration; ++onboardingRevision; ++readGeneration; ++generation; clearInterval(interval) }
    },
    connectWifi: (networkId: string, password?: string) => { void connect(networkId, false, password) },
    connectHiddenWifi: (ssid: string, password?: string) => { void connect(ssid, true, password) },
    scanWifi() {
      if (!active || busy) return
      busy = true
      const current = ++generation
      const network = state.network
      publish({ ...state, problem: undefined, ...(network?.wifi._tag === "Available" ? { network: { ...network, wifi: { ...network.wifi, scanning: true } } } : {}) })
      void korrid.scanWifi().then(outcome => {
        if (!active || current !== generation) return
        if (outcome._tag === "Ok") publish({ ...state, network: networkFrom(outcome.payload) })
        else publish({ ...state, problem: "Could not scan for networks. Try again." })
      }).catch(() => { if (active && current === generation) publish({ ...state, problem: FAILURE }) })
        .finally(() => {
          if (!active || current !== generation) return
          busy = false
          const network = state.network
          if (network?.wifi._tag === "Available") publish({ ...state, network: { ...network, wifi: { ...network.wifi, scanning: false } } })
        })
    },
    finishOnboarding() {
      // Finishing setup is independent of a radio scan or activation.
      if (!active || finishing) return
      finishing = true
      ++onboardingRevision
      const current = ++finishGeneration
      void korrid.finishOnboarding().then(outcome => {
        if (!active || current !== finishGeneration) return
        ++onboardingRevision
        publish(outcome._tag === "Ok" ? { ...state, onboardingNeeded: outcome.payload.needed, problem: undefined }
          : { ...state, problem: "Could not save setup progress. Try again." })
      }).catch(() => { if (active && current === finishGeneration) publish({ ...state, problem: FAILURE }) })
        .finally(() => { if (current === finishGeneration) finishing = false })
    },
    dismissProblem() { publish({ ...state, problem: undefined }) },
  }
}
