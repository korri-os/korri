/**
 * What Velvet remembers changes nothing but time (src/memo.ts).
 *
 * `update`, `view` and every selection under them give the same answer for
 * the same arguments, whatever was asked before. Velvet keeps answers so it
 * does not work them out again, and a kept answer that is found by less than
 * everything it depends on gives a different answer after a different
 * history: the home's row once stayed as it was when a game crossed the
 * seven-day line, because the kept row was found by the newest game alone.
 *
 * Held here:
 *
 *  - The gate: every binding at the top of a module in src/ (and logic.ts)
 *    that can change is named in an `onForget` of its file. So nothing is
 *    kept that `forget()` does not empty, and nothing is kept in a way this
 *    file does not see.
 *  - The property: each selection, called with its memory warm from a
 *    history of other calls and again with its memory empty, answers the
 *    same. For the leaves, over generated arguments in two orders; for
 *    `update` and `view`, at every step of every scenario and of generated
 *    plays in which Korri's time moves games across the week's line.
 */
import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import { blankTile, gameCard, gameTile, marqueeOf, navTile, pictures, setTile, stageOf, systemTile } from "../src/art"
import { nearest, rowItems } from "../src/browse"
import { stageLight } from "../src/colour"
import { capsWidth, fitCaps, textWidth } from "../src/fit"
import { freshGames, homeOf, LISTS_START, PICK_DAYS, shelvesOf, wants } from "../src/lists"
import { forget } from "../src/memo"
import { plan } from "../src/plan"
import { posterFor } from "../src/poster"
import { qrDataUrl } from "../src/qr"
import type { Game, Lists, State } from "../src/types"
import { PACKAGE, Play, scenario, scenarios, SIZES } from "./korri"

/** A small generator with a seed: the same numbers every run. */
function numbers(seed: number): () => number {
  let state = seed >>> 0
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0
    return state / 0x100000000
  }
}

const DAY = 86_400_000
const NOW = 1_794_000_000_000

/**
 * `ask`, for every argument: with the memory warm from all the calls before
 * it, with the memory warm from the calls in the other order, and with the
 * memory empty. The three answers are equal by value.
 */
function sameWhateverCameBefore<A>(inputs: readonly A[], ask: (input: A) => unknown): void {
  forget()
  const warm = inputs.map(input => JSON.stringify(ask(input)))
  forget()
  const otherOrder = [...inputs].reverse().map(input => JSON.stringify(ask(input))).reverse()
  const cold = inputs.map(input => {
    forget()
    return JSON.stringify(ask(input))
  })
  for (let i = 0; i < inputs.length; i++) {
    expect(warm[i], `argument ${i}, warm`).toBe(cold[i]!)
    expect(otherOrder[i], `argument ${i}, warm from the other order`).toBe(cold[i]!)
  }
}

describe("the gate: everything kept can be forgotten", () => {
  const files = ["logic.ts", ...readdirSync(join(PACKAGE, "src")).filter(name => name.endsWith(".ts")).map(name => `src/${name}`)]

  test("every binding at a module's top that can change is named in an onForget of its file", () => {
    const loose: string[] = []
    let kept = 0
    for (const file of files) {
      const source = readFileSync(join(PACKAGE, file), "utf8")
      // `let` and `var`; a Map, a Set or a WeakMap; an array or an object that is not read-only.
      const names = [
        ...[...source.matchAll(/^(?:export )?(?:let|var) (\w+)/gm)].map(match => match[1]!),
        ...[...source.matchAll(/^(?:export )?const (\w+)(?:: [^=\n]+)? = new (?:Map|Set|WeakMap|WeakSet)\b/gm)].map(match => match[1]!),
        ...[...source.matchAll(/^(?:export )?const (\w+)(: [^=\n]+)? = (?:\[\]|\{\})$/gm)].filter(match => !/readonly|Readonly/.test(match[2] ?? "")).map(match => match[1]!),
      ]
      if (file === "src/memo.ts") continue
      const forgets = [...source.matchAll(/onForget\(\(\) => (\{[\s\S]*?\n\}\)|[^\n]*\))/g)].map(match => match[1]!).join("\n")
      for (const name of names) {
        kept += 1
        if (!new RegExp(`\\b${name}\\b`).test(forgets)) loose.push(`${file}: ${name}`)
      }
    }
    expect(loose).toEqual([])
    // The sweep sees what is there (a pattern that matched nothing would hold nothing).
    expect(kept).toBeGreaterThan(25)
  })

  test("no WeakMap: it cannot be emptied, so it cannot be held to the rule", () => {
    for (const file of files) expect(/new Weak(Map|Set)\b/.test(readFileSync(join(PACKAGE, file), "utf8")), file).toBe(false)
  })
})

describe("the home's lists: the same lists and the same time give the same row", () => {
  const game = (id: string, daysAgo: number | undefined, seconds = 0): Game => ({
    id, title: `Game ${id}`, ...(daysAgo === undefined ? {} : { lastPlayedAt: NOW - Math.round(daysAgo * DAY), playCount: 1 }), totalPlaytimeSeconds: seconds,
  })
  const lists = (kept: Record<string, Game[]>): Lists => ({ ...LISTS_START, kept, total: 100, read: ["recent"] })
  const said = (home: ReturnType<typeof homeOf>) => ({ pick: home.pick?.id, why: home.why, row: home.row.map(entry => entry.id) })

  test("the reviewer's case: A stays this week's, B crosses the seven days, and the row follows", () => {
    // Most played, in order: c, b. This week's games lead the row; a game that left the week stands by its playtime.
    const a = game("a", 1, 10)
    const b = game("b", PICK_DAYS - 0.5, 50)
    const c = game("c", 30, 900)
    const held = lists({ recent: [a, b, c], most: [c, b, a] })
    forget()
    expect(said(homeOf(held, NOW)).row).toEqual(["b", "c"])
    // A day on, B is eight days old. The memory is warm from the call above.
    const warm = said(homeOf(held, NOW + DAY))
    forget()
    const cold = said(homeOf(held, NOW + DAY))
    expect(warm).toEqual(cold)
    expect(cold.row).toEqual(["c", "b"])
    expect(cold.pick).toBe("a")
    // And back: the same arguments as the first call, the same answer.
    expect(said(homeOf(held, NOW)).row).toEqual(["b", "c"])
  })

  test("generated lists and times, forwards and backwards across every game's line", () => {
    const next = numbers(7)
    for (let round = 0; round < 40; round++) {
      const count = 3 + Math.floor(next() * 12)
      const recent = Array.from({ length: count }, (_, i) => game(`r${i}`, next() < 0.2 ? undefined : next() * 14, Math.floor(next() * 1000)))
        .sort((x, y) => (y.lastPlayedAt ?? -Infinity) - (x.lastPlayedAt ?? -Infinity))
      const most = [...recent].filter(entry => (entry.totalPlaytimeSeconds ?? 0) > 0).sort((x, y) => y.totalPlaytimeSeconds! - x.totalPlaytimeSeconds!)
      const held = lists({ recent, most, korri: [...recent].reverse() })
      // Times a few hours apart over three weeks, visited in a shuffled order; and no time at all.
      const times: (number | undefined)[] = Array.from({ length: 30 }, () => NOW - 7 * DAY + Math.floor(next() * 21 * DAY))
      times.push(undefined, NOW, undefined)
      sameWhateverCameBefore(times, time => said(homeOf(held, time)))
    }
  })

  test("the exact line: a session a week old to the millisecond is this week's, one millisecond more is not", () => {
    const edge = game("edge", PICK_DAYS, 5)
    const other = game("other", 40, 500)
    const held = lists({ recent: [edge, other], most: [other, edge] })
    sameWhateverCameBefore([NOW - 1, NOW, NOW + 1, NOW, NOW + 2, NOW - 1], time => said(homeOf(held, time)))
    // The row's places are made from that answer, and follow it.
    sameWhateverCameBefore([NOW, NOW + 1, NOW, NOW + 2], time => rowItems({ lists: held } as State, { publishedAt: time } as any).map(item => (item.kind === "game" ? item.game.id : "all")))
    forget()
    expect(said(homeOf(held, NOW)).pick).toBe("edge")
    expect(said(homeOf(held, NOW + 1)).pick).toBe("other")
  })

  test("the shelves, the games not yet started and the questions: by everything they are made of", () => {
    const next = numbers(11)
    const model = (systems: number) => ({ systems: Array.from({ length: systems }, (_, i) => ({ systemId: `s${i}`, system: `System ${i}`, total: 1 + i })) }) as any
    const inputs: { lists: Lists; systems: number }[] = []
    for (let round = 0; round < 30; round++) {
      const recent = Array.from({ length: 8 }, (_, i) => game(`r${round}-${i}`, i < 4 + (round % 5) ? 1 + i : undefined, 10 * i))
      const tail = Array.from({ length: 6 }, (_, i) => game(`t${round}-${i}`, round % 3 === 0 && i < 2 ? 50 : undefined))
      const kept = { recent, tail, fresh: tail, "peer:zao": [{ ...game(`z${round}`, 9), section: "zao" }] }
      // The same kept lists under different peers, totals and pages: each makes its own answer.
      const base: Lists = { ...LISTS_START, kept, total: 50 + round, read: ["recent"] }
      inputs.push({ lists: base, systems: round % 4 })
      inputs.push({ lists: { ...base, peers: ["zao"], peerTotals: { zao: 3 + (round % 9) } }, systems: round % 4 })
      inputs.push({ lists: { ...base, peers: ["zao"], peerTotals: { zao: 40 } }, systems: (round + 1) % 4 })
      // Pages to read from both ends, and the same pages once the first game not yet started is found.
      const paged: Lists = { ...base, kept: { ...kept, recent: recent.slice(0, 3), tail: tail.slice(3) }, scans: [{ cursor: "i49", index: 50 }], tails: [{ cursor: "i20", index: 20 }] }
      inputs.push({ lists: paged, systems: 0 })
      inputs.push({ lists: { ...paged, found: `t${round}-3` }, systems: 0 })
      inputs.push({ lists: { ...base, total: Math.floor(next() * 1000) }, systems: 1 })
    }
    const ids = (shelves: ReturnType<typeof shelvesOf>) => shelves.map(shelf => [shelf.key, shelf.label, shelf.items.map(item => (item.kind === "game" ? item.game.id : item.kind === "system" ? `${item.systemId}:${item.count}` : `${item.label}:${item.count ?? ""}`))])
    sameWhateverCameBefore(inputs, input => ids(shelvesOf(input.lists, model(input.systems))))
    sameWhateverCameBefore(inputs, input => freshGames(input.lists).map(entry => entry.id))
    sameWhateverCameBefore(inputs, input => wants(input.lists))
  })
})

describe("the leaves: by their whole arguments, exactly", () => {
  const titles = ["Ember Road", "Tide Pool", "The Long Winter of the Clockmaker's Apprentice", "µCity", "Ys Ⅷ", "x", "", "A\u0000B", "\u0001strong", "straße"]

  test("a title fitted to a box: two boxes that differ by a hair are two fits", () => {
    const boxes: [number, number, number, number][] = []
    // Narrow boxes, where the width decides the size; wide ones, where the height or the most does.
    // (The widths that are near each other are asked one after the other: a fit is kept for a few boxes a title.)
    for (const height of [24, 24.4, 60]) for (const max of [12, 12.3, 40]) for (const min of [4.2, 4.4]) for (const width of [20, 20.3, 19.7, 20.0625, 52, 52.45, 120]) boxes.push([width, height, max, min])
    sameWhateverCameBefore(titles.flatMap(title => boxes.map(box => [title, ...box] as const)), ([title, width, height, max, min]) => fitCaps(title, width, height, max, min))
  })

  test("widths, in each face and weight", () => {
    sameWhateverCameBefore(titles, title => capsWidth(title))
    sameWhateverCameBefore(titles.flatMap(title => [[title, false], [title, true]] as const), ([title, strong]) => textWidth(title, strong))
  })

  test("the stage light and the no-art card: no two games share one by the way their names are joined", () => {
    const colours = [undefined, "", "#b0452a", "#808080", "#000000", "nonsense", "a\u0000b"]
    sameWhateverCameBefore(colours.flatMap(colour => titles.map(title => [colour, title] as const)), ([colour, title]) => stageLight(colour, title))
    // "a" with the title "b\0c" and "a\0b" with the title "c" would be one key if the two were joined by a mark.
    sameWhateverCameBefore([["a", "b\u0000c"], ["a\u0000b", "c"], ["a", "b\u0000c"]] as const, ([colour, title]) => stageLight(colour, title))
    const systems = [undefined, "", "gba", "GBA ", "nes", "wasm4", "a\u0000b", "a"]
    sameWhateverCameBefore(systems.flatMap(system => titles.map(title => [title, system] as const)), ([title, system]) => posterFor(title, system))
    sameWhateverCameBefore([["b\u0000c", "a"], ["c", "a\u0000b"]] as const, ([title, system]) => posterFor(title, system))
  })

  test("the order of places, the plan, the code of a backup", () => {
    const places: [number, number][] = []
    for (const count of [0, 1, 2, 6, 14, 4096, 4097]) for (const from of [0, 1, 5, 13, 4095, 4096]) if (count <= 20 || from === 0) places.push([count, from])
    places.push([1, 4096], [2, 0])
    sameWhateverCameBefore(places, ([count, from]) => nearest(count, from))
    sameWhateverCameBefore([...SIZES, [640, 481], [641, 480], [640, 480]] as const, ([width, height]) => plan(width, height))
    sameWhateverCameBefore(["nsec1abc", "nsec1abd", "", "nsec1abc"], text => qrDataUrl(text).length + qrDataUrl(text).slice(-24))
  })

  test("a tile: by its game, its place and the pictures its view has asked for", () => {
    const next = numbers(3)
    const art = scenario("home").models[0].catalog.games as Game[]
    const games: Game[] = Array.from({ length: 40 }, (_, i) => ({ ...art[i % art.length]!, id: `g${i % 25}`, title: `${art[i % art.length]!.title} ${i % 25}`, ...(i >= 25 ? { resumable: true, system: "Other" } : {}) }))
    const calls = Array.from({ length: 300 }, () => ({
      game: games[Math.floor(next() * games.length)]!, at: Math.floor(next() * 6), focused: next() < 0.3, shelf: Math.floor(next() * 3), rests: next() < 0.5,
      region: next() < 0.5 ? "row" : "shelf:lately", budget: next() < 0.2 ? 30 : 0,
    }))
    /** Pictures with `spent` megabytes already asked for: a view that is near its budget, or not. */
    const asked = (megabytes: number) => ({ ...pictures(), spent: megabytes * 1024 * 1024, taken: new Set<string>() })
    // The same focus id for different places on purpose: a place is found by its id and held to every argument.
    sameWhateverCameBefore(calls, call => gameTile(asked(call.budget), call.game, call.region, `t:${call.at % 3}`, call.focused, call.at, { shelf: call.shelf, rests: call.rests }))
    sameWhateverCameBefore(calls, call => setTile(asked(call.budget), call.game, call.at, call.focused, call.at % 4))
    sameWhateverCameBefore(calls, call => gameCard(asked(call.budget), call.game))
    sameWhateverCameBefore(calls, call => navTile(call.focused ? "See all" : "All games", String(call.at), call.region, `n:${call.at % 2}`, call.focused, call.at, { rests: call.rests }))
    sameWhateverCameBefore(calls, call => systemTile(`s${call.at}`, `System ${call.shelf}`, `${call.at} games`, call.region, `s:${call.at % 2}`, call.focused, call.at))
    sameWhateverCameBefore(calls, call => stageOf(asked(call.budget), call.game, 0.3, call.focused))
    sameWhateverCameBefore(calls, call => marqueeOf(asked(call.budget), call.game, { width: 200 + call.at, logoH: 80, title: 40, fact: 12, mark: call.rests, badge: call.focused }, NOW))
    expect(blankTile(3)).toEqual(blankTile(3))
  })
})

describe("update and view: the same state, message and model, the same answer", () => {
  /**
   * Play, with the memory left warm the whole way, and keep every `update`
   * and every `view` that was asked with its answer. Then ask each one again
   * with the memory empty, and hold the two answers equal. Returns how many
   * were compared.
   */
  function compared(play: (run: (make: () => Play) => Play) => void): number {
    const real = { update: globalThis.surface.update, view: globalThis.surface.view }
    const asked: { readonly what: string; readonly again: () => unknown; readonly warm: string }[] = []
    globalThis.surface.update = (state: any, message: any, model: any) => {
      const warm = real.update(state, message, model)
      asked.push({ what: `update ${JSON.stringify(message)}`, again: () => real.update(state, message, model), warm: JSON.stringify(warm) })
      return warm
    }
    const look = (run: Play): void => {
      const state = run.state
      const model = run.model
      asked.push({ what: "view", again: () => real.view(state, model), warm: JSON.stringify(real.view(state, model)) })
    }
    forget()
    try {
      play(make => {
        const run = make()
        const step = run.step.bind(run)
        const swap = run.swap.bind(run)
        run.step = (name, models) => {
          step(name, models)
          look(run)
        }
        run.swap = model => {
          swap(model)
          look(run)
        }
        look(run)
        return run
      })
    } finally {
      globalThis.surface.update = real.update
    }
    asked.forEach((call, index) => {
      forget()
      expect(JSON.stringify(call.again()), `call ${index}, ${call.what}`).toBe(call.warm)
    })
    return asked.length
  }

  test("every scenario, every step, at two sizes", () => {
    const count = compared(start => {
      for (const scene of scenarios()) {
        for (const [width, height] of [[1280, 720], [480, 800]] as const) {
          const run = start(() => new Play(scene.models[0], width, height))
          for (const step of scene.steps) run.step(step, scene.models)
        }
      }
    })
    expect(count).toBeGreaterThan(1500)
  }, 120_000)

  test("generated plays: presses, ticks, and Korri's time moving games across the week's line", () => {
    const base = scenario("home").models[0]
    const { systems: _systems, ...rest } = base
    const next = numbers(23)
    const buttons = ["up", "down", "left", "right", "confirm", "back", "options", "down", "right", "down"]
    const count = compared(start => {
      for (let round = 0; round < 6; round++) {
        // 120 games: sessions spread over three weeks around the line, some never played, a few on another device.
        const games = Array.from({ length: 120 }, (_, i) => ({
          id: `g${i}`, title: `Game ${String(i).padStart(3, "0")}`, section: i % 17 === 0 ? "zao" : "This device", systemId: `s${i % 5}`, system: `System ${i % 5}`,
          ...(next() < 0.25 ? {} : { lastPlayedAt: base.publishedAt - Math.floor(next() * 21 * DAY), playCount: 1 + (i % 7), totalPlaytimeSeconds: Math.floor(next() * 90_000) }),
        }))
        let library = { ...rest, catalog: { _tag: "Ready", games } }
        const run = start(() => new Play(library, round % 2 === 0 ? 1280 : 640, round % 2 === 0 ? 720 : 480))
        let time = base.publishedAt
        for (let step = 0; step < 120; step++) {
          const roll = next()
          if (roll < 0.65) run.step(buttons[Math.floor(next() * buttons.length)]!, [library])
          else if (roll < 0.75) run.step(`wait ${Math.floor(next() * 5)}`, [library])
          else if (roll < 0.9) {
            // Korri's clock moves on by hours or days; the games are the same.
            time += Math.floor(next() * 2 * DAY)
            run.swap({ ...library, publishedAt: time, clockLabel: `clock ${time}` })
          } else {
            // Korri changes a game and publishes: its artwork is stored, or it was just played.
            const at = Math.floor(next() * games.length)
            const changed = next() < 0.5
              ? { ...games[at]!, media: [{ role: "poster", url: `preview/art/poster-${at}-${step}.png`, width: 600, height: 900, color: "#804020" }] }
              : { ...games[at]!, lastPlayedAt: time, playCount: 9, totalPlaytimeSeconds: 100_000 + step }
            games[at] = changed
            library = { ...rest, catalog: { _tag: "Ready", games: [...games] }, publishedAt: time, clockLabel: `clock ${time}` }
            run.swap(library)
          }
        }
      }
    })
    expect(count).toBeGreaterThan(1500)
  }, 120_000)
})
