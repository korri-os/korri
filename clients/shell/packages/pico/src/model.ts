/**
 * The treaty as Pico's own code reads it.
 *
 * Every type is Korri's (`contracts/generated/surface.ts`), with one change:
 * Pico's code was written when the catalog was a whole list, so it reads
 * `catalog.games`. Korri now publishes a cursor window (contract 3.4); the
 * adapter in `logic.ts` hands Pico the window as that list (`picoModelFrom`).
 * Whole-list work (Find's query, section and order) goes to Korri as
 * `MoveCatalog`, so the window is always the list Pico would have drawn.
 */
import type {
  SurfaceCatalog as KorriCatalog,
  SurfaceGame,
  SurfaceModel as KorriModel,
} from "../../../../../contracts/generated/surface"

export type * from "../../../../../contracts/generated/surface"

/** The catalog as Pico reads it: the window Korri sent, as `games`. */
export type SurfaceCatalog =
  | { readonly _tag: "Loading" }
  | { readonly _tag: "Error"; readonly message: string }
  | { readonly _tag: "Empty" }
  | {
      readonly _tag: "Ready"
      readonly games: readonly SurfaceGame[]
      /** The window holds every game of its query: a game missing from it
       * is gone. Absent (a whole list, as Pico's fixtures give): complete. */
      readonly complete?: boolean
    }

export type SurfaceModel = Omit<KorriModel, "catalog"> & { readonly catalog: SurfaceCatalog }

export type { KorriCatalog, KorriModel }

const cache = new WeakMap<KorriModel, SurfaceModel>()

/** Korri's model with its window handed over as Pico's list. One object per
 * model, so Pico's per-catalog caches keep working. */
export function picoModelFrom(model: KorriModel): SurfaceModel {
  const cached = cache.get(model)
  if (cached !== undefined) return cached
  const catalog: SurfaceCatalog = model.catalog._tag === "Ready"
    ? {
        _tag: "Ready",
        games: model.catalog.window,
        complete: model.catalog.before === undefined && model.catalog.after === undefined,
      }
    : model.catalog
  const shaped: SurfaceModel = { ...model, catalog }
  cache.set(model, shaped)
  return shaped
}
