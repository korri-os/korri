import type { SurfaceStarterPack } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

const PACK_ID = "@korri:starter-pack"
const INSTALL_ACTION = "starter-pack-install"
const FAILURE = "Could not install the Starter pack. Try again."

/** One offer per setup session. No persisted state beyond the host's receipt. */
export function createStarterPackOnboarding(korrid: KorridClient) {
  let state: SurfaceStarterPack | undefined
  let needed = false
  let generation = 0
  const listeners = new Set<() => void>()
  const publish = (next: SurfaceStarterPack | undefined) => {
    state = next
    const subscribers = [...listeners]
    for (const listener of subscribers) listener()
  }
  const failed = (): SurfaceStarterPack => ({
    _tag: "Failed", message: FAILURE,
    actions: [{ id: INSTALL_ACTION, label: "Retry", enabled: true }],
  })
  const install = async () => {
    const current = generation
    publish({ _tag: "Installing" })
    try {
      const enabled = await korrid.setPluginEnabled(PACK_ID, true)
      if (!needed || current !== generation) return
      if (enabled._tag === "Err") { publish(failed()); return }
      const rescanned = await korrid.rescanDiscovery()
      if (!needed || current !== generation) return
      publish(rescanned._tag === "Ok" ? { _tag: "Installed" } : failed())
    } catch {
      // Host refusal text is not user-facing copy and must not reach logs.
      if (needed && current === generation) publish(failed())
    }
  }
  const read = async (current: number) => {
    try {
      const outcome = await korrid.pluginRepositories()
      if (!needed || current !== generation) return
      if (outcome._tag === "Ok" && outcome.payload.otherInstalls.some(install => install.id === PACK_ID && !install.enabled)) {
        publish({ _tag: "Offered", actions: [{ id: INSTALL_ACTION, label: "Install", enabled: true }] })
      }
    } catch { /* Unavailable hosts and failed reads make no offer. */ }
  }
  return {
    snapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener) } },
    setNeeded(next: boolean) {
      if (needed === next) return
      needed = next
      const current = ++generation
      publish(undefined)
      if (needed) void read(current)
    },
    act(id: string): boolean {
      if (id !== INSTALL_ACTION) return false
      if (needed && (state?._tag === "Offered" || state?._tag === "Failed")) void install()
      return true
    },
  }
}
