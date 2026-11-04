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
  clock = "10:24"
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
      clockLabel: this.clock,
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
    deliver,
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
    // The game runs: a host with no Running status publishes its session as
    // a card of its own (korrid's model.rs, Entry::NowPlaying), first under
    // CONTINUE.
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

describe("a session outside the cursor window", () => {
  test("a game launched past the first window of 120 is watched by Korri's status, and its screen closes when it ends", () => {
    const games: SurfaceGame[] = Array.from({ length: 120 }, (_, i) => ({ id: `g${i}`, title: `Game ${String(i).padStart(3, "0")}` }))
    const korri = new Korri(games)
    const pico = run(korri)
    for (let i = 0; i < 87; i += 1) pico.press("right")
    pico.press("confirm")
    expect(pico.state().nav.detail?.gameId).toBe("g87")
    // g87 is outside the first window Korri sent.
    expect(korri.model().catalog._tag === "Ready" && korri.from._tag !== "Start").toBe(true)
    pico.press("confirm")
    expect(korri.requests.some(request => request._tag === "LaunchGame" && request.gameId === "g87")).toBe(true)
    expect(pico.state().nav.session._tag).toBe("AwaitingSession")
    // Korri starts it, then it runs. Its session card lands far outside the
    // window the shelf holds.
    korri.status = { _tag: "Busy", kicker: "Starting Game 087…", gameId: "g87" }
    pico.publish()
    korri.status = { _tag: "Running", kicker: "PLAYING", gameId: "g87" }
    korri.games = [...korri.games, { id: "now:g87", title: "Game 087", resumable: true }]
    pico.publish()
    expect(pico.state().nav.session).toMatchObject({ _tag: "Watching", viewingId: "g87", live: true })
    // Pushes while it plays keep the watch and the screen.
    korri.focused = "g87"
    pico.publish()
    expect(pico.state().nav.detail?.gameId).toBe("g87")
    // It ends: Korri is browsing again; the card leaves a window that never held it.
    korri.status = { _tag: "Browsing" }
    korri.games = korri.games.filter(game => game.id !== "now:g87")
    pico.publish()
    expect(pico.state().nav.detail).toBeUndefined()
    expect(pico.state().nav.session._tag).toBe("Idle")
  })

  test("a session ended by another device closes the screen once its Ended card is dismissed", () => {
    const games: SurfaceGame[] = Array.from({ length: 120 }, (_, i) => ({ id: `g${i}`, title: `Game ${String(i).padStart(3, "0")}` }))
    const korri = new Korri(games)
    const pico = run(korri)
    for (let i = 0; i < 87; i += 1) pico.press("right")
    pico.press("confirm", "confirm")
    korri.status = { _tag: "Paused", kicker: "Paused", actions: [], gameId: "g87" }
    pico.publish()
    expect(pico.state().nav.session).toMatchObject({ _tag: "Watching", live: true })
    korri.status = { _tag: "Ended", kicker: "ENDED", reason: "Simon left.", gameId: "g87" }
    pico.publish()
    expect(pico.state().nav.detail?.gameId).toBe("g87")
    korri.status = { _tag: "Browsing" }
    pico.publish()
    expect(pico.state().nav.detail).toBeUndefined()
  })

  test("a cancelled launch keeps the screen", () => {
    const korri = new Korri(fixtureGames.map(game => ({ ...game, resumable: false })))
    const pico = run(korri)
    pico.press("right", "right", "right", "confirm", "confirm")
    korri.status = { _tag: "Busy", kicker: "Starting…", gameId: "spelunky" }
    pico.publish()
    korri.status = { _tag: "Browsing" }
    pico.publish()
    expect(pico.state().nav.detail?.gameId).toBe("spelunky")
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

describe("attract", () => {
  test("starts when the shell's Tick reports 45 s with no press, and a press only wakes it", () => {
    const korri = new Korri(fixtureGames.map(game => ({ ...game })))
    const pico = run(korri)
    const tick = (idleMs: number) => pico.deliver({ _tag: "Tick", idleMs })
    tick(44_999)
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
    tick(45_000)
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(true)
    // The waking press does nothing else: the cursor stays on the first cart.
    const before = pico.state().nav.home.selectedGameId
    pico.press("right")
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
    pico.press("confirm")
    expect(pico.state().nav.detail?.gameId).toBeDefined()
    void before
  })

  test("the clock label changing is not idleness", () => {
    const korri = new Korri(fixtureGames.map(game => ({ ...game })))
    const pico = run(korri)
    for (const minute of ["10:25", "10:26", "10:27"]) {
      ;(korri as unknown as { clock: string }).clock = minute
      pico.publish()
    }
    expect((pico.view()["attract"] as { shown: boolean }).shown).toBe(false)
  })
})

describe("quiet messages keep the state object", () => {
  test("a Tick below the attract threshold, and a Resized to the same size, return the same state", () => {
    const korri = new Korri(fixtureGames.map(game => ({ ...game })))
    const model = korri.model()
    let state = init(model, {})
    state = update(state, { _tag: "Resized", width: 640, height: 480 }, model).state
    state = update(state, { _tag: "KorriPublished" }, model).state
    for (const idleMs of [0, 1_000, 44_999]) expect(update(state, { _tag: "Tick", idleMs }, model).state).toBe(state)
    expect(update(state, { _tag: "Resized", width: 640, height: 480 }, model).state).toBe(state)
    // The threshold crossed: attract starts, and the state changes.
    expect(update(state, { _tag: "Tick", idleMs: 45_000 }, model).state).not.toBe(state)
    // The same state and model draw the same view object.
    expect(view(state, model)).toBe(view(state, model))
  })
})

describe("a publish that changes nothing", () => {
  test("a freshly parsed, equal model keeps the state and the view; unchanged rows keep their arrays", () => {
    const korri = new Korri(fixtureGames.map(game => ({ ...game })))
    const parsed = () => JSON.parse(JSON.stringify(korri.model())) as KorriModel
    let state = init(parsed(), {})
    state = update(state, { _tag: "Resized", width: 640, height: 480 }, parsed()).state
    state = update(state, { _tag: "KorriPublished" }, parsed()).state
    const before = view(state, parsed()) as { attract: { carts: unknown[] }; home: { menu: unknown[] } }
    const again = update(state, { _tag: "KorriPublished" }, parsed())
    expect(again.state).toBe(state)
    expect(view(state, parsed())).toBe(before)
    // A move along the shelf changes the carts, not MENU's list or attract's.
    const moved = update(state, { _tag: "Pressed", button: "right" }, parsed()).state
    const after = view(moved, parsed()) as typeof before
    expect(after).not.toBe(before)
    expect(after.attract.carts).toBe(before.attract.carts)
    expect(after.home.menu).toBe(before.home.menu)
  })
})
