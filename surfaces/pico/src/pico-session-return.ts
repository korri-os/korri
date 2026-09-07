import type { SurfaceCatalog, SurfaceModel } from "@contracts/surface/korri-surface"

type ReadyCatalog = Extract<SurfaceCatalog, { readonly _tag: "Ready" }>

export type PicoSessionReturn =
  | { readonly _tag: "Idle" }
  | { readonly _tag: "AwaitingSession"; readonly viewingId: string; readonly catalog: ReadyCatalog }
  | { readonly _tag: "Watching"; readonly viewingId: string; readonly sessionId: string }
  | { readonly _tag: "ReturnToLibrary" }

/** A request alone is not evidence that anything played. */
export function picoSessionReturnOnPlay(
  previous: PicoSessionReturn,
  gameId: string,
  catalog: SurfaceCatalog,
): PicoSessionReturn {
  if (previous._tag === "Watching" && previous.viewingId === gameId) return previous
  if (catalog._tag !== "Ready") return { _tag: "Idle" }
  return catalog.games.some(game => game.id === gameId && game.resumable === true)
    ? { _tag: "Watching", viewingId: gameId, sessionId: gameId }
    : { _tag: "AwaitingSession", viewingId: gameId, catalog }
}

/**
 * The host publishes a separate resumable card after a successful Play, not a
 * Running status. Associate only one newly observed card with that request.
 * The treaty has no launch-to-session link or completion reason: multiple new
 * cards, replacements and unavailable catalogs are not proof of completion.
 * A fresh Browsing/Ready with no remaining session is the host's evidence here;
 * this cannot distinguish completion from a host incorrectly dropping truth.
 */
export function picoSessionReturnFromModel(
  previous: PicoSessionReturn,
  viewingId: string | undefined,
  model: Pick<SurfaceModel, "catalog" | "status">,
): PicoSessionReturn {
  const idle: PicoSessionReturn = { _tag: "Idle" }
  if (viewingId === undefined) return idle
  const state = "viewingId" in previous && previous.viewingId === viewingId ? previous : idle
  const { catalog, status } = model
  if (status._tag === "Problem" && state._tag === "AwaitingSession") return idle
  if (catalog._tag !== "Ready" || status._tag !== "Browsing") return state
  const resumable = catalog.games.filter(game => game.resumable === true)

  if (state._tag === "Watching") {
    if (catalog.games.some(game => game.id === state.sessionId)) return state
    return resumable.length === 0 ? { _tag: "ReturnToLibrary" } : idle
  }
  if (state._tag === "AwaitingSession") {
    if (catalog === state.catalog) return state
    const added = resumable.filter(game => !state.catalog.games.some(old => old.id === game.id))
    const session = added.length === 1 ? added[0] : undefined
    return session === undefined ? idle : { _tag: "Watching", viewingId, sessionId: session.id }
  }
  return resumable.some(game => game.id === viewingId)
    ? { _tag: "Watching", viewingId, sessionId: viewingId }
    : idle
}
