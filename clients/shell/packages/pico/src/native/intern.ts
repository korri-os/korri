/**
 * Korri's model, kept by identity across publishes.
 *
 * The shell parses Korri's model afresh on every publish. Parts equal to the
 * last model's keep the last model's objects, and a model equal in every
 * part is the last model itself: the screen, the view and every cart built
 * from it are reused, not rebuilt.
 *
 * Telling equal parts apart is most of a publish's cost in QuickJS, so it is
 * done the cheapest way that is still exact: each game of the catalog's
 * window against the last window's game of the same id, field by field
 * (`sameGame`); every other part by its JSON text, written in C. Only a part
 * that differs is walked (`share`), to keep what is equal inside it.
 */
import type { KorriModel, SurfaceGame, SurfaceLaunchLocation } from "../model"
import { share } from "./memo"

let lastIn: KorriModel | undefined
let lastOut: KorriModel | undefined
let partTexts = new Map<string, string | undefined>()
let lastGames = new Map<string, SurfaceGame>()

export function interned(korri: KorriModel): KorriModel {
  if (korri === lastIn || korri === lastOut) return lastOut as KorriModel
  const before = lastOut as Record<string, unknown> | undefined
  const record = korri as unknown as Record<string, unknown>
  const texts = new Map<string, string | undefined>()
  const out: Record<string, unknown> = {}
  let same = before !== undefined
  let keys = 0
  for (const key in record) {
    keys += 1
    const part = record[key]
    let kept: unknown
    if (key === "catalog") {
      kept = catalogInterned(before?.["catalog"] as KorriModel["catalog"] | undefined, part as KorriModel["catalog"])
    } else {
      const text = JSON.stringify(part)
      texts.set(key, text)
      kept = before !== undefined && key in before && partTexts.has(key) && partTexts.get(key) === text ? before[key] : share(before?.[key], part)
    }
    if (before === undefined || kept !== before[key]) same = false
    out[key] = kept
  }
  if (same && before !== undefined) for (const _ in before) keys -= 1
  const result = same && keys === 0 ? lastOut as KorriModel : out as unknown as KorriModel
  partTexts = texts
  lastIn = korri
  lastOut = result
  return result
}

/** The catalog, each game of its window kept from the last window when it is the same. */
function catalogInterned(before: KorriModel["catalog"] | undefined, catalog: KorriModel["catalog"]): KorriModel["catalog"] {
  if (catalog._tag !== "Ready") return share(before, catalog)
  const games = new Map<string, SurfaceGame>()
  const window = catalog.window.map(game => {
    const known = lastGames.get(game.id)
    const kept = known !== undefined && sameGame(known, game) ? known : game
    games.set(game.id, kept)
    return kept
  })
  lastGames = games
  return share(before, { ...catalog, window })
}

/**
 * Every field of a game. A field the treaty adds fails the typecheck here
 * until `sameGame` compares it too (test/native-program.test.ts changes each
 * one).
 */
export const GAME_FIELDS: Readonly<Record<keyof SurfaceGame, true>> = {
  id: true,
  title: true,
  launchLocations: true,
  section: true,
  subtitle: true,
  coverArtUrl: true,
  resumable: true,
  activeProfile: true,
  lastPlayedAt: true,
  playCount: true,
  totalPlaytimeSeconds: true,
  saves: true,
  download: true,
}

/** Every field of a launch location, as `GAME_FIELDS` (`sameLocations`). */
export const LOCATION_FIELDS: Readonly<Record<keyof SurfaceLaunchLocation, true>> = { id: true, label: true }

/** Whether two games are equal: field by field, by name (QuickJS reads a
 * named field several times faster than a field looked up by a string). */
export function sameGame(a: SurfaceGame, b: SurfaceGame): boolean {
  return a.id === b.id
    && a.title === b.title
    && a.section === b.section
    && a.subtitle === b.subtitle
    && a.coverArtUrl === b.coverArtUrl
    && a.resumable === b.resumable
    && a.activeProfile === b.activeProfile
    && a.lastPlayedAt === b.lastPlayedAt
    && a.playCount === b.playCount
    && a.totalPlaytimeSeconds === b.totalPlaytimeSeconds
    && (a.launchLocations === b.launchLocations || sameLocations(a.launchLocations, b.launchLocations))
    // Rarely present: by their JSON text.
    && (a.saves === b.saves || sameText(a.saves, b.saves))
    && (a.download === b.download || sameText(a.download, b.download))
}

function sameLocations(a: SurfaceGame["launchLocations"], b: SurfaceGame["launchLocations"]): boolean {
  if (a === undefined || b === undefined || a.length !== b.length) return false
  for (let i = 0; i < a.length; i += 1) {
    const x = a[i]
    const y = b[i]
    if (x === undefined || y === undefined || x.id !== y.id || x.label !== y.label) return false
  }
  return true
}

function sameText(a: unknown, b: unknown): boolean {
  return a !== undefined && b !== undefined && JSON.stringify(a) === JSON.stringify(b)
}
