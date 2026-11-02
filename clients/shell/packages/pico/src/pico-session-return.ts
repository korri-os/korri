import type { SurfaceCatalog, SurfaceModel } from "./model"

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
    if (sameSessions(catalog, state.catalog)) return state
    const added = resumable.filter(game => {
      const before = state.catalog.games.find(old => old.id === game.id)
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
