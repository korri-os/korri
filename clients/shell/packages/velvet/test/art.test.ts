/**
 * ADR 0091: Korri sends each image's kind and size, and Velvet reads a
 * picture's use from its shape, r = width / height (contract slice 4):
 *
 *   background  1.6 <= r <= 1.9 (near 16:9)
 *   wide hero   r >= 2.0
 *   poster      0.6 <= r <= 0.75
 *   square tile 0.85 <= r <= 1.2
 *
 * Within a class the largest in pixels wins. The stage takes the background,
 * else the wide hero, and cuts either to fill at every size. A tile takes the
 * poster, else the square tile or Korri's coverArtUrl, else the logo, else
 * the no-art card.
 */
import { describe, expect, test } from "bun:test"
import "../logic"
import type { SurfaceMedia } from "../../../../../contracts/generated/surface"
import { chooseArt, classOf, decodedBytes, fitting, gameCard, gameTile, hasHero, marqueeOf, PIXEL_BUDGET, pictures, setTile, stageOf } from "../src/art"
import { forget } from "../src/memo"
import type { Game } from "../src/types"
import { Play, scenario } from "./korri"

const picture = (name: string, width: number, height: number, color?: string): SurfaceMedia =>
  ({ kind: "picture", url: `preview/art/${name}.png`, width, height, ...(color === undefined ? {} : { color }) })
const logo = (name: string, width: number, height: number): SurfaceMedia => ({ kind: "logo", url: `preview/art/${name}.png`, width, height })
let serial = 0
const game = (media: SurfaceMedia[], more: Partial<Game> = {}): Game => ({ id: `art-${serial++}`, title: "Ember Road", section: "This device", media, ...more } as Game)

describe("a picture's class is its shape", () => {
  const cases: [number, number, string | undefined][] = [
    [1600, 1000, "background"], [1920, 1080, "background"], [1900, 1000, "background"],
    [1590, 1000, undefined], [1910, 1000, undefined], [1950, 1000, undefined],
    [2000, 1000, "wide"], [1920, 620, "wide"], [3840, 600, "wide"],
    [600, 1000, "poster"], [600, 900, "poster"], [750, 1000, "poster"],
    [590, 1000, undefined], [760, 1000, undefined], [840, 1000, undefined],
    [850, 1000, "square"], [512, 512, "square"], [1200, 1000, "square"],
    [1210, 1000, undefined], [1590, 1000, undefined], [300, 1000, undefined],
    [0, 1000, undefined], [1000, 0, undefined],
  ]
  for (const [width, height, expected] of cases) {
    test(`${width} x ${height} (${(width / Math.max(1, height)).toFixed(3)}): ${expected ?? "none"}`, () => {
      expect(classOf(width, height)).toBe(expected as any)
    })
  }
})

describe("within a class the largest wins", () => {
  test("of two backgrounds, the one with more pixels; of two the same size, the first", () => {
    const small = picture("bg-small", 1280, 720)
    const big = picture("bg-big", 1920, 1080)
    const twin = picture("bg-twin", 1920, 1080)
    expect(chooseArt([small, big, twin]).background).toBe(big)
    expect(chooseArt([twin, small, big]).background).toBe(twin)
  })

  test("each class keeps its own: background, wide, poster, square; and the largest logo", () => {
    const media = [
      picture("hero", 1920, 620), picture("hero-small", 1600, 500), picture("bg", 1920, 1080),
      picture("poster", 600, 900), picture("poster-big", 1000, 1500), picture("square", 512, 512),
      logo("logo-small", 400, 160), logo("logo", 800, 320),
    ]
    const art = chooseArt(media)
    expect(art.background?.url).toBe("preview/art/bg.png")
    expect(art.wide?.url).toBe("preview/art/hero.png")
    expect(art.poster?.url).toBe("preview/art/poster-big.png")
    expect(art.square?.url).toBe("preview/art/square.png")
    expect(art.logo?.url).toBe("preview/art/logo.png")
    expect(art.stage?.url).toBe("preview/art/bg.png")
  })

  test("a logo is never a picture, an icon is neither, and an image with no address is not there", () => {
    const art = chooseArt([
      { kind: "logo", url: "preview/art/wide-logo.png", width: 2400, height: 600 },
      { kind: "icon", url: "preview/art/icon.png", width: 512, height: 512 },
      { ...picture("empty", 1920, 1080), url: "" },
    ])
    expect(art.wide).toBeUndefined()
    expect(art.square).toBeUndefined()
    expect(art.background).toBeUndefined()
    expect(art.logo?.url).toBe("preview/art/wide-logo.png")
  })

  test("a picture between two classes is for nothing", () => {
    expect(chooseArt([picture("between", 1400, 1000), picture("almost", 1950, 1000)])).toEqual({})
  })
})

describe("the stage", () => {
  const sizes: [number, number][] = [[320, 240], [480, 800], [390, 844], [640, 480], [1280, 720], [1920, 1080], [2560, 1080]]

  test("a background is the stage, over a wide hero, cut to fill at every size", () => {
    forget()
    const one = game([picture("hero", 1920, 620, "#204060"), picture("bg", 1920, 1080, "#b0452a")])
    for (const [width, height] of sizes) {
      const stage = stageOf(pictures(), one, 0, false, false, width, height)
      expect(stage.artShown).toBe(true)
      expect(stage.art).toEqual({ $image: "preview/art/bg.png" })
      expect(stage.fill).toBe(true)
    }
    expect(hasHero(one)).toBe(true)
  })

  test("with no background, the wide hero, cut to fill at every size", () => {
    forget()
    const one = game([picture("hero", 1920, 620), picture("poster", 600, 900)])
    for (const [width, height] of sizes) {
      const stage = stageOf(pictures(), one, 0, false, false, width, height)
      expect(stage.art).toEqual({ $image: "preview/art/hero.png" })
      expect(stage.fill).toBe(true)
    }
  })

  test("the light takes the stage picture's colour first", () => {
    forget()
    const withBackground = game([picture("hero", 1920, 620, "#204060"), picture("bg", 1920, 1080, "#b0452a"), picture("poster", 600, 900, "#40a040")])
    const withHero = game([picture("hero", 1920, 620, "#204060"), picture("poster", 600, 900, "#40a040")])
    const a = stageOf(pictures(), withBackground, 0, false)
    const b = stageOf(pictures(), withHero, 0, false)
    const c = stageOf(pictures(), game([picture("bg", 1920, 1080, "#b0452a")], { title: withHero.title }), 0, false)
    expect(a.light).toBe(c.light)
    expect(a.light).not.toBe(b.light)
  })

  test("a poster or a square picture alone never lights the stage: the light alone", () => {
    forget()
    const one = game([picture("poster", 600, 900), picture("square", 512, 512)])
    expect(hasHero(one)).toBe(false)
    const stage = stageOf(pictures(), one, 0, false, false, 1280, 720)
    expect(stage.artShown).toBe(false)
  })

  test("far from the game (lit), its colour without its picture", () => {
    forget()
    const stage = stageOf(pictures(), game([picture("bg", 1920, 1080)]), 0.8, false, true, 1280, 720)
    expect(stage.artShown).toBe(false)
  })
})

describe("the tile and the mark", () => {
  test("the fallback order: poster, then a square tile or coverArtUrl, then the logo, then the no-art card", () => {
    forget()
    const cover = "preview/art/cover-ember.png"
    const card = (media: SurfaceMedia[], more: Partial<Game> = {}) => gameCard(pictures(), game(media, more))
    expect(card([picture("poster", 600, 900), picture("square", 512, 512), logo("logo", 800, 320)], { coverArtUrl: cover })).toMatchObject({ artKind: "poster", art: { $image: "preview/art/poster.png" } })
    expect(card([picture("square", 512, 512), logo("logo", 800, 320)], { coverArtUrl: cover })).toMatchObject({ artKind: "cover", art: { $image: "preview/art/square.png" } })
    expect(card([logo("logo", 800, 320)], { coverArtUrl: cover })).toMatchObject({ artKind: "cover", art: { $image: cover } })
    expect(card([picture("bg", 1920, 1080), logo("logo", 800, 320)])).toMatchObject({ artKind: "logo", art: { $image: "preview/art/logo.png" } })
    expect(card([picture("bg", 1920, 1080), picture("hero", 1920, 620)])).toMatchObject({ artKind: "none" })
  })

  test("the mark uses the largest logo", () => {
    forget()
    const one = game([logo("logo-small", 400, 160), logo("logo", 800, 320)])
    const mark = marqueeOf(pictures(), one, { width: 800, logoH: 200, title: 80, fact: 20, mark: true, badge: false }, undefined)
    expect(mark.logo).toEqual({ $image: "preview/art/logo.png" })
  })
})

describe("through the view", () => {
  test("home: Ember Road's background is the stage, in one window and on the Stage of two; the Floor cuts the row's hero to fill", () => {
    for (const [width, height] of [[1920, 1080], [390, 844], [320, 240]] as const) {
      const one = new Play(scenario("home").models[0], width, height)
      expect(one.view.home.mark.key).toBe("ember-road")
      expect(one.view.stage.art).toEqual({ $image: "preview/art/background-ember.png" })
      expect(one.view.stage.fill).toBe(true)
      const two = new Play({ ...scenario("home").models[0], windows: [{ panes: ["stage"] }, { panes: ["floor"] }] }, width, height)
      two.deliver({ _tag: "Resized", width, height, window: 1 } as any)
      const stage = globalThis.surface.view(two.state, two.model, 0) as any
      const floor = globalThis.surface.view(two.state, two.model, 1) as any
      expect(stage.stage.art).toEqual({ $image: "preview/art/background-ember.png" })
      // The Floor shows the row, focused on the next game: Tide Pool, a wide hero alone.
      expect(floor.stage.art).toEqual({ $image: "preview/art/hero-tide.png" })
      expect(floor.stage.fill).toBe(true)
    }
  })

  test("the row on a portrait window: Tide Pool's wide hero, cut to fill", () => {
    const run = new Play(scenario("home").models[0], 480, 800)
    run.press("down")
    expect(run.view.stage.art).toEqual({ $image: "preview/art/hero-tide.png" })
    expect(run.view.stage.fill).toBe(true)
  })
})

describe("a picture is chosen for the size it is drawn at, and the budget picks the picture, not the layout", () => {
  test("fitting: the least that covers the box first, then each smaller one; with no box, the largest first", () => {
    const sorted = [{ width: 200, height: 300 }, { width: 400, height: 600 }, { width: 600, height: 900 }, { width: 1000, height: 1500 }]
    expect(fitting(sorted, 300, 450).map(one => one.height)).toEqual([600, 300])
    expect(fitting(sorted, 600, 900).map(one => one.height)).toEqual([900, 600, 300])
    expect(fitting(sorted, 2000, 3000).map(one => one.height)).toEqual([1500, 900, 600, 300])
    expect(fitting(sorted, 0, 0).map(one => one.height)).toEqual([1500, 900, 600, 300])
    expect(fitting([], 100, 100)).toEqual([])
  })

  test("a tile shows the least poster as large as the tile; a larger tile, a larger poster; no size known, the largest", () => {
    forget()
    const one = game([picture("poster-small", 300, 450), picture("poster-big", 1000, 1500), picture("poster-mid", 600, 900)])
    expect(chooseArt(one.media).posters?.map(poster => poster.height)).toEqual([450, 900, 1500])
    expect(gameCard(pictures(), one, 400)).toMatchObject({ artKind: "poster", art: { $image: "preview/art/poster-small.png" } })
    expect(gameCard(pictures(), one, 600)).toMatchObject({ art: { $image: "preview/art/poster-mid.png" } })
    expect(gameCard(pictures(), one, 1300)).toMatchObject({ art: { $image: "preview/art/poster-big.png" } })
    expect(gameCard(pictures(), one, 4000)).toMatchObject({ art: { $image: "preview/art/poster-big.png" } })
    expect(gameCard(pictures(), one)).toMatchObject({ art: { $image: "preview/art/poster-big.png" } })
  })

  test("where the budget cannot hold the poster that fits, a smaller one; where it holds none, the no-art card", () => {
    forget()
    const one = game([picture("poster-small", 300, 450), picture("poster-big", 1000, 1500)])
    const near = (left: number) => ({ ...pictures(), spent: PIXEL_BUDGET - left })
    expect(setTile(near(decodedBytes(1000, 1500)), one, 0, false, 0, 1400)).toMatchObject({ art: { $image: "preview/art/poster-big.png" } })
    expect(setTile(near(decodedBytes(1000, 1500) - 1), one, 0, false, 0, 1400)).toMatchObject({ art: { $image: "preview/art/poster-small.png" } })
    expect(setTile(near(decodedBytes(300, 450) - 1), one, 0, false, 0, 1400)).toMatchObject({ artKind: "none" })
  })

  test("a poster the view already holds and that covers the tile costs nothing more", () => {
    forget()
    const one = game([picture("poster-small", 300, 450), picture("poster-big", 1000, 1500)])
    const asked = pictures()
    gameTile(asked, one, "row", "row:0", false, 0, undefined, false, 1400)
    const spent = asked.spent
    expect(gameTile(asked, one, "shelf:x", "shelf:x:0", false, 0, undefined, false, 300)).toMatchObject({ art: { $image: "preview/art/poster-big.png" } })
    expect(asked.spent).toBe(spent)
  })

  test("the stage takes the least background that covers the window", () => {
    forget()
    const one = game([picture("bg-small", 1280, 720), picture("bg-big", 3840, 2160)])
    expect(stageOf(pictures(), one, 0, false, false, 1280, 720).art).toEqual({ $image: "preview/art/bg-small.png" })
    expect(stageOf(pictures(), one, 0, false, false, 1920, 1080).art).toEqual({ $image: "preview/art/bg-big.png" })
    expect(stageOf(pictures(), one, 0, false, false, 3840, 2160).art).toEqual({ $image: "preview/art/bg-big.png" })
  })
})
