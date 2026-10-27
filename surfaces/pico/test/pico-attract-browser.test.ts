/**
 * Attract and waking from it, in Chromium, on the RG DS displays and on one.
 *
 * The idle timer is Pico's own (PICO_ATTRACT_AFTER_MS); the page's clock is
 * moved past it. Input goes through the portal's own input path (its bus and
 * spatial focus, test/browser/attract.tsx), and the CPU is throttled six
 * times to come nearer the RK3568's pace. A tap is a pointer press.
 *
 * Attract says PRESS ANY BUTTON: every input ends it. After every step the
 * cursor (DOM focus) and Pico's chosen cart (the showcase) are the same cart.
 *
 * Set CHROMIUM to a Chromium executable; without it the tests are skipped.
 */
import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import type { SurfaceInputAction } from "@contracts/surface/korri-surface"
import { type Browser, chromium, type Page } from "playwright-core"
import { PICO_ATTRACT_AFTER_MS } from "../src/pico-attract"
import { serveBrowserPages } from "./browser/serve"

const executablePath = process.env.CHROMIUM
const run = executablePath === undefined || executablePath === "" ? test.skip : test

let server: Awaited<ReturnType<typeof serveBrowserPages>> | undefined
let browser: Browser | undefined

beforeAll(async () => {
  if (executablePath === undefined || executablePath === "") return
  server = await serveBrowserPages()
  browser = await chromium.launch({ executablePath })
}, 30_000)

afterAll(async () => {
  await browser?.close()
  server?.stop()
})

/** What the page draws now. Without displays, the whole screen is `choose`. */
interface Drawn {
  readonly attract: { readonly show: boolean; readonly choose: boolean }
  /** The game Pico has chosen, as its stage names it. */
  readonly chosen: string | null
  /** The shelf cart DOM focus is on: its index, and its name. */
  readonly cursor: { readonly index: number; readonly name: string } | null
  readonly menuOpen: boolean
}

async function open(displays: "rgds" | "none"): Promise<Page> {
  if (browser === undefined || server === undefined) throw new Error("no browser or server")
  const page = await browser.newPage({ viewport: { width: 640, height: displays === "rgds" ? 960 : 480 } })
  const cdp = await page.context().newCDPSession(page)
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 6 })
  await page.clock.install()
  await page.goto(new URL(`test/browser/attract.html${displays === "none" ? "?displays=none" : ""}`, server.address).href)
  await page.waitForFunction(() => typeof window.picoDirection === "function")
  await page.waitForSelector(".pico-cart-shelf-rack button")
  return page
}

const drawn = (page: Page) => page.evaluate((): Drawn => {
  const show = document.querySelector("[data-pico-display='show']")
  const choose = document.querySelector("[data-pico-display='choose']") ?? document.querySelector(".pico-catalog-surface")
  const visible = (scope: Element | null) => {
    const element = scope?.querySelector<HTMLElement>(".pico-attract")
    return element !== null && element !== undefined && element.getClientRects().length > 0
  }
  const stage = show?.querySelector(".pico-game-stage") ?? document.querySelector(".pico-cart-shelf-stage .pico-game-stage")
  const carts = [...document.querySelectorAll<HTMLButtonElement>(".pico-cart-shelf-rack button")]
  const index = carts.indexOf(document.activeElement as HTMLButtonElement)
  return {
    attract: { show: visible(show), choose: visible(choose) },
    chosen: stage?.getAttribute("aria-label") ?? null,
    cursor: index === -1 ? null : { index, name: carts[index]?.getAttribute("aria-label") ?? "" },
    menuOpen: document.querySelector(".pico-menu-list") !== null,
  }
})

/** Let the throttled page finish what the last input started. */
const settle = async (page: Page) => {
  await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
  await page.waitForTimeout(100)
}

async function idle(page: Page) {
  await page.clock.fastForward(PICO_ATTRACT_AFTER_MS + 1_000)
  await settle(page)
  const now = await drawn(page)
  expect(now.attract.show || now.attract.choose).toBe(true)
}

const direction = async (page: Page, to: "left" | "right" = "right") => {
  await page.evaluate(d => window.picoDirection(d), to)
  await settle(page)
}
const confirm = async (page: Page) => {
  await page.evaluate(() => window.picoConfirm())
  await settle(page)
}
const button = async (page: Page, action: SurfaceInputAction) => {
  await page.evaluate(a => window.picoPress(a), action)
  await settle(page)
}

/** The invariant: the cursor is on a shelf cart, and that cart is the chosen one. */
async function agreed(page: Page): Promise<Drawn & { readonly cursor: NonNullable<Drawn["cursor"]> }> {
  const now = await drawn(page)
  expect(now.attract).toEqual({ show: false, choose: false })
  expect(now.cursor).not.toBeNull()
  expect(now.chosen).not.toBeNull()
  expect(now.cursor?.name.startsWith(`${now.chosen},`) === true || now.cursor?.name === now.chosen).toBe(true)
  return now as Drawn & { readonly cursor: NonNullable<Drawn["cursor"]> }
}

for (const displays of ["rgds", "none"] as const) {
  const where = displays === "rgds" ? "two displays" : "one display"

  describe(`waking from attract on ${where}`, () => {
    run("one direction wakes; the cursor and the chosen cart agree after it and after every later press", async () => {
      const page = await open(displays)
      await direction(page)
      const start = await agreed(page)
      await idle(page)

      await direction(page)
      let now = await agreed(page)
      expect(now.cursor.index).toBe(start.cursor.index + 1)
      for (const step of [1, 2, 3]) {
        await direction(page)
        const next = await agreed(page)
        expect(next.cursor.index).toBe(now.cursor.index + 1)
        now = next
        expect(step).toBeGreaterThan(0)
      }
      await direction(page, "left")
      const back = await agreed(page)
      expect(back.cursor.index).toBe(now.cursor.index - 1)
      // Confirm opens the chosen game: its own screen names it.
      await confirm(page)
      const opened = await page.evaluate(() => document.querySelector(".pico-status-bar-label")?.textContent ?? "")
      expect(back.chosen).toBe(opened)
      await page.close()
    }, 60_000)

    for (const [name, wake] of [
      ["confirm", confirm],
      ["back", (page: Page) => button(page, "back")],
      ["menu", (page: Page) => button(page, "menu")],
    ] as const) {
      run(`${name} wakes and changes nothing else; the next direction moves one cart`, async () => {
        const page = await open(displays)
        await direction(page)
        const start = await agreed(page)
        await idle(page)
        await wake(page)
        const woke = await agreed(page)
        expect(woke.cursor.index).toBe(start.cursor.index)
        expect(woke.menuOpen).toBe(false)
        await direction(page)
        expect((await agreed(page)).cursor.index).toBe(start.cursor.index + 1)
        await page.close()
      }, 60_000)
    }
  })
}

describe("touch and MENU on two displays", () => {
  for (const region of ["show", "choose"] as const) {
    run(`a tap on the ${region === "show" ? "main" : "choosing"} display wakes and changes nothing else`, async () => {
      const page = await open("rgds")
      await direction(page)
      const start = await agreed(page)
      await idle(page)
      await page.mouse.click(320, region === "show" ? 240 : 720)
      await settle(page)
      const woke = await agreed(page)
      expect(woke.cursor.index).toBe(start.cursor.index)
      expect(woke.menuOpen).toBe(false)
      await page.close()
    }, 60_000)
  }

  run("waking, then opening MENU, leaves no attract on either display", async () => {
    const page = await open("rgds")
    await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-menu-key")?.focus())
    await idle(page)
    await confirm(page)
    await confirm(page)
    const after = await drawn(page)
    expect(after.menuOpen).toBe(true)
    expect(after.attract).toEqual({ show: false, choose: false })
    expect(after.chosen).toBe("Celeste Classic")
    await page.close()
  }, 60_000)
})
