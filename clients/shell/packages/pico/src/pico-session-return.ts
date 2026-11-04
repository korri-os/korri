import type { SurfaceCatalog, SurfaceModel } from "./model"

type ReadyCatalog = Extract<SurfaceCatalog, { readonly _tag: "Ready" }>

export type PicoSessionReturn =
  | { readonly _tag: "Idle" }
  /** Play was pressed. `catalog` is the window then, when Korri had one. */
  | { readonly _tag: "AwaitingSession"; readonly viewingId: string; readonly catalog?: ReadyCatalog }
  /**
   * A session of the game is under way. `live`: Korri's status (or the
   * gameplay overlay) has shown it, so its return to browsing ends it,
   * whatever the cursor window holds.
   */
  | { readonly _tag: "Watching"; readonly viewingId: string; readonly sessionId: string; readonly live?: true }
  | { readonly _tag: "ReturnToLibrary" }

/** A request alone is not evidence that anything played. */
export function picoSessionReturnOnPlay(
  previous: PicoSessionReturn,
  gameId: string,
  catalog: SurfaceCatalog,
): PicoSessionReturn {
  if (previous._tag === "Watching" && previous.viewingId === gameId) return previous
  if (catalog._tag !== "Ready") return { _tag: "AwaitingSession", viewingId: gameId }
  return catalog.games.some(game => game.id === gameId && game.resumable === true)
    ? { _tag: "Watching", viewingId: gameId, sessionId: gameId }
    : { _tag: "AwaitingSession", viewingId: gameId, catalog }
}

/**
 * A session's evidence, first: Korri's status Running or Paused for the game
 * (or the gameplay overlay), then its return to Browsing. Hosts that publish
 * no Running status show a session as a separate resumable card after Play:
 * associate only one newly observed card with that request.
 * The treaty has no launch-to-session link or completion reason: multiple new
 * cards, replacements and unavailable catalogs are not proof of completion.
 * A fresh Browsing/Ready with no remaining session is the host's evidence here;
 * this cannot distinguish completion from a host incorrectly dropping truth.
 */
export function picoSessionReturnFromModel(
  previous: PicoSessionReturn,
  viewingId: string | undefined,
  model: Pick<SurfaceModel, "catalog" | "status"> & Partial<Pick<SurfaceModel, "presentation">>,
): PicoSessionReturn {
  const idle: PicoSessionReturn = { _tag: "Idle" }
  if (viewingId === undefined) return idle
  const state = "viewingId" in previous && previous.viewingId === viewingId ? previous : idle
  const { catalog, status } = model

  // The launch's own evidence, independent of the browsing window (treaty:
  // Running and Paused carry the live launch; the gameplay overlay is shown
  // only over a running game). A game's absence from a window proves nothing.
  const live = ((status._tag === "Running" || status._tag === "Paused") && (status.gameId === undefined || status.gameId === viewingId))
    || model.presentation?.kind === "gameplay-overlay"
  if (live && (state._tag === "AwaitingSession" || state._tag === "Watching")) {
    return state._tag === "Watching" && state.live === true ? state : { _tag: "Watching", viewingId, sessionId: state._tag === "Watching" ? state.sessionId : viewingId, live: true }
  }
  if (state._tag === "Watching" && state.live === true) {
    // Seen live: it ends when Korri is browsing again (after an Ended card,
    // once dismissed). Starting, stopping and failure screens wait.
    return status._tag === "Browsing" && model.presentation?.kind !== "gameplay-overlay" ? { _tag: "ReturnToLibrary" } : state
  }
  if (status._tag === "Problem" && state._tag === "AwaitingSession") return idle
  if (catalog._tag !== "Ready" || status._tag !== "Browsing") return state
  const resumable = catalog.games.filter(game => game.resumable === true)

  // A cursor window that does not hold every game says nothing about a game
  // it leaves out: only a game seen without its session, or a complete
  // window without it, is evidence.
  const complete = catalog.complete !== false

  if (state._tag === "Watching") {
    const session = catalog.games.find(game => game.id === state.sessionId)
    if (session === undefined && !complete) return state
    if (session !== undefined) return state
    return resumable.length === 0 ? { _tag: "ReturnToLibrary" } : idle
  }
  if (state._tag === "AwaitingSession") {
    // A new model is not a new catalog: Korri republishes for many reasons
    // (the focused game's actions, the clock). Only a change in the games
    // that resume counts.
    const then = state.catalog
    if (then === undefined) return state
    if (sameSessions(catalog, then)) return state
    const added = resumable.filter(game => {
      const before = then.games.find(old => old.id === game.id)
      return before === undefined ? complete : before.resumable !== true
    })
    const session = added.length === 1 ? added[0] : undefined
    return session === undefined ? idle : { _tag: "Watching", viewingId, sessionId: session.id }
  }
  return resumable.some(game => game.id === viewingId)
    ? { _tag: "Watching", viewingId, sessionId: viewingId }
    : idle
}

/** The same games resume in both catalogs, by id, where both hold them. */
function sameSessions(a: ReadyCatalog, b: ReadyCatalog): boolean {
  if (a === b) return true
  const resumes = (catalog: ReadyCatalog) => new Map(catalog.games.map(game => [game.id, game.resumable === true]))
  const left = resumes(a)
  const right = resumes(b)
  for (const [id, on] of left) if (right.has(id) && right.get(id) !== on) return false
  for (const [id, on] of right) if (on && !left.has(id) && a.complete !== false) return false
  for (const [id, on] of left) if (on && !right.has(id) && b.complete !== false) return false
  return true
}
