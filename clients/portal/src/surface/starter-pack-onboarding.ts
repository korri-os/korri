import type { LibraryFoldersSnapshotOutcome } from "@contracts/generated/korrid"
import type { SurfaceStarterPack } from "@contracts/surface/korri-surface"
import type { KorridClient } from "../korrid/client"

const PACK_ID = "@korri:starter-pack"
const INSTALL_ACTION = "starter-pack-install"
const FAILURE = "Could not install the Starter pack. Try again."

export interface StarterPackOnboardingOptions {
  readonly pollIntervalMs?: number
  readonly discoveryTimeoutMs?: number
  /** Refresh host facts even if the person has already finished setup. */
  readonly onInstalled?: () => void
}

/** One offer per setup session. No persisted state beyond the host's receipt. */
export function createStarterPackOnboarding(korrid: KorridClient, options: StarterPackOnboardingOptions = {}) {
  let state: SurfaceStarterPack | undefined
  let needed = false
  let mounted = true
  let busy = false
  let packEnabled = false
  let generation = 0
  let cancelWait: (() => void) | undefined
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
  const waitForDiscovery = async (initial: LibraryFoldersSnapshotOutcome) => {
    if (!mounted) return
    let pollTimer: ReturnType<typeof setTimeout> | undefined
    let cancel!: () => void
    const cancelled = new Promise<never>((_, reject) => { cancel = () => reject(new Error("Discovery wait ended")) })
    cancelWait = cancel
    const deadline = setTimeout(cancel, options.discoveryTimeoutMs ?? 60_000)
    let outcome = initial
    try {
      for (;;) {
        if (outcome._tag === "Err") throw new Error("Discovery failed")
        switch (outcome.payload.state._tag) {
          case "Idle": return
          case "Problem": throw new Error("Discovery failed")
          case "Scanning":
          case "Enriching": break
        }
        await Promise.race([
          new Promise<void>(resolve => { pollTimer = setTimeout(resolve, options.pollIntervalMs ?? 250) }),
          cancelled,
        ])
        outcome = await Promise.race([korrid.libraryFoldersSnapshot(), cancelled])
      }
    } finally {
      clearTimeout(deadline)
      clearTimeout(pollTimer)
      if (cancelWait === cancel) cancelWait = undefined
    }
  }
  const install = async (retry: boolean) => {
    if (busy) return
    busy = true
    const current = generation
    const visible = () => mounted && needed && current === generation
    publish({ _tag: "Installing" })
    try {
      // A lost enable reply may still have committed. Retry reads the receipt
      // before repeating that effect; an acknowledged enable is already known.
      if (retry && !packEnabled) {
        const repositories = await korrid.pluginRepositories()
        if (repositories._tag === "Err") throw new Error("Could not read installed plugins")
        packEnabled = repositories.payload.otherInstalls.find(install => install.id === PACK_ID)?.enabled === true
      }
      if (!packEnabled) {
        const enabled = await korrid.setPluginEnabled(PACK_ID, true)
        if (enabled._tag === "Err") throw new Error("Enable failed")
        packEnabled = true
      }
      // Finishing setup hides this operation, but never cancels its effects.
      const rescanned = await korrid.rescanLibraryFolders()
      await waitForDiscovery(rescanned)
      if (mounted) options.onInstalled?.()
      if (visible()) publish({ _tag: "Installed" })
    } catch {
      // Host refusal text is not user-facing copy and must not reach logs.
      if (visible()) publish(failed())
    } finally { busy = false }
  }
  const read = async (current: number) => {
    try {
      const outcome = await korrid.pluginRepositories()
      if (!mounted || !needed || current !== generation) return
      if (outcome._tag === "Ok" && outcome.payload.otherInstalls.find(install => install.id === PACK_ID)?.enabled === false) {
        packEnabled = false
        publish({ _tag: "Offered", actions: [{ id: INSTALL_ACTION, label: "Install", enabled: true }] })
      }
    } catch { /* Unavailable hosts and failed reads make no offer. */ }
  }
  return {
    snapshot: () => state,
    subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener) } },
    start() {
      mounted = true
      return () => {
        mounted = false
        needed = false
        ++generation
        cancelWait?.()
        state = undefined
      }
    },
    setNeeded(next: boolean) {
      if (!mounted || needed === next) return
      needed = next
      const current = ++generation
      publish(undefined)
      if (needed) void read(current)
    },
    act(id: string): boolean {
      if (id !== INSTALL_ACTION) return false
      if (needed && (state?._tag === "Offered" || state?._tag === "Failed")) void install(state._tag === "Failed")
      return true
    },
  }
}
