/**
 * The native adapter (src/native/program.ts) against a small korrid of its
 * own: the cursor window over a list of games (contract 3.4), FocusGame
 * publishing the focused game's actions, and every request answered by a
 * fresh model, as the shell delivers korrid's pushes.
 */
import { describe, expect, test } from "bun:test"
import type { CatalogFrom, CatalogQuery, KorriModel, SurfaceGame, SurfaceRequest } from "../src/model"
import { init, update, view } from "../src/native/program"
import type { State } from "../src/native/types"
import { fixtureModel } from "./fixtures/fixture-host"

/** A korrid in miniature: windows, focus, and the status a test sets. */
class Korri {
  query: CatalogQuery = { text: "", order: "korri" }
  from: CatalogFrom = { _tag: "Start" }
  count = 50
  focused: string | undefined
  status: KorriModel["status"] = { _tag: "Browsing" }
  requests: SurfaceRequest[] = []
  constructor(public games: SurfaceGame[]) {}

  model(): KorriModel {
    const { query } = this
    const needle = query.text.toLowerCase()
    let list = this.games.filter(game => game.title.toLowerCase().includes(needle) && (query.section == null || game.section === query.section))
    if (query.order === "title") list = [...list].sort((a, b) => a.title.localeCompare(b.title))
    if (query.order === "recent") list = [...list].sort((a, b) => (b.lastPlayedAt ?? -1) - (a.lastPlayedAt ?? -1))
    let first = 0
    if (this.from._tag === "Around") {
      const id = this.from.gameId
      const at = Math.max(0, list.findIndex(game => game.id === id))
      first = Math.max(0, Math.min(at - Math.floor(this.count / 2), list.length - this.count))
    }
    const window = list.slice(first, first + this.count)
    const base = fixtureModel as unknown as KorriModel
    return {
      ...base,
      status: this.status,
      catalog: {
        _tag: "Ready",
        total: list.length,
        window,
        firstIndex: first,
        ...(first > 0 ? { before: `b${first}` } : {}),
        ...(first + this.count < list.length ? { after: `a${first + this.count}` } : {}),
        query,
      },
      ...(this.focused === undefined ? {} : { focusedGame: { gameId: this.focused, actions: [] } }),
    } as KorriModel
  }

  answer(requests: readonly SurfaceRequest[]): boolean {
    let changed = false
    for (const request of requests) {
      this.requests.push(request)
      if (request._tag === "MoveCatalog") {
        this.query = request.query
        this.from = request.from
        this.count = request.count
        changed = true
      }
      if (request._tag === "FocusGame") {
        this.focused = request.gameId
        changed = true
      }
    }
    return changed
  }
}

/** Run the package as the shell does: a message, then korrid's pushes. */
function run(korri: Korri) {
  let state: State = init(korri.model(), {})
  const deliver = (message: Parameters<typeof update>[1]) => {
    let next = update(state, message, korri.model())
    state = next.state
    while (korri.answer(next.requests)) {
      next = update(state, { _tag: "KorriPublished" }, korri.model())
      state = next.state
    }
  }
  deliver({ _tag: "KorriPublished" })
  return {
    press: (...buttons: readonly string[]) => {
      for (const button of buttons) deliver({ _tag: "Pressed", button } as Parameters<typeof update>[1])
    },
    publish: () => deliver({ _tag: "KorriPublished" }),
    state: () => state,
    view: () => view(state, korri.model()) as Record<string, unknown>,
  }
}

const fixtureGames = (fixtureModel.catalog._tag === "Ready" ? fixtureModel.catalog.games : []) as SurfaceGame[]

describe("the session a game's own screen watches", () => {
  test("a push that only changes the focused game keeps the watch, and the screen closes when the game ends", () => {
    // Spelunky: no launch choice. No session runs yet.
    const korri = new Korri(fixtureGames.map(game => ({ ...game, resumable: false })))
    const pico = run(korri)
    pico.press("right", "right", "right", "confirm")
    expect(pico.state().nav.detail?.gameId).toBe("spelunky")
    pico.press("confirm")
    expect(korri.requests.some(request => request._tag === "LaunchGame" && request.gameId === "spelunky")).toBe(true)
    expect(pico.state().nav.session._tag).toBe("AwaitingSession")
    // Korri republishes with only the focused game's actions changed.
    korri.focused = "spelunky"
    pico.publish()
    pico.publish()
    expect(pico.state().nav.session._tag).toBe("AwaitingSession")
    // The game runs: korrid publishes its session as a card of its own
    // (model.rs, Entry::NowPlaying), first under CONTINUE.
    korri.status = { _tag: "Running", kicker: "PLAYING", gameId: "spelunky" }
    pico.publish()
    korri.status = { _tag: "Browsing" }
    korri.games = [{ id: "now:spelunky", title: "Spelunky", section: "Continue", resumable: true }, ...korri.games]
    pico.publish()
    expect(pico.state().nav.session).toMatchObject({ _tag: "Watching", sessionId: "now:spelunky" })
    // A push with only the focused game changed keeps watching.
    korri.focused = "spelunky"
    pico.publish()
    expect(pico.state().nav.session._tag).toBe("Watching")
    // The session ends: its card is gone from a window that holds every game.
    korri.games = korri.games.filter(game => game.id !== "now:spelunky")
    pico.publish()
    expect(pico.state().nav.detail).toBeUndefined()
  })
})

describe("the hero", () => {
  test("is the game played last in the whole library, outside the first window", () => {
    const games: SurfaceGame[] = Array.from({ length: 120 }, (_, i) => ({
      id: `g${i}`,
      title: `Game ${String(i).padStart(3, "0")}`,
      lastPlayedAt: i === 87 ? 9_000_000 : i < 40 ? 1000 + i : undefined,
      ...(i === 87 || i === 3 ? { resumable: true } : {}),
    }))
    const korri = new Korri(games)
    const pico = run(korri)
    pico.press("menu", "menu")
    expect(pico.state().nav.home.mode).toBe("hero")
    expect(korri.requests.some(request => request._tag === "MoveCatalog" && request.query.order === "recent")).toBe(true)
    const home = pico.view()["home"] as { hero: { title: string; resume: { title: string }[] } }
    expect(home.hero.title).toBe("Game 087")
    expect(home.hero.resume.map(game => game.title)).toContain("Game 087")
  })
})
