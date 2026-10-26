/**
 * Attract and waking from it, in Chromium, on the RG DS displays and on one.
 *
 * The idle timer is Pico's own (PICO_ATTRACT_AFTER_MS); the page's clock is
 * moved past it. Input arrives as the portal delivers it: a direction moves
 * DOM focus and nothing else, confirm clicks the focused control, and Back
 * and Menu are the host's buttons.
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

/** What each region draws now. Without displays, the whole screen is `choose`. */
interface Drawn {
  readonly attract: { readonly show: boolean; readonly choose: boolean }
  /** The showcase's heading on the main display; null when none shows. */
  readonly showcase: string | null
  readonly menuOpen: boolean
}

async function open(displays: "rgds" | "none"): Promise<Page> {
  if (browser === undefined || server === undefined) throw new Error("no browser or server")
  const page = await browser.newPage({ viewport: { width: 640, height: displays === "rgds" ? 960 : 480 } })
  await page.clock.install()
  await page.goto(new URL(`test/browser/attract.html${displays === "none" ? "?displays=none" : ""}`, server.address).href)
  await page.waitForFunction(() => typeof window.picoPress === "function")
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
  return {
    attract: { show: visible(show), choose: visible(choose) },
    showcase: show?.querySelector<HTMLElement>(".pico-game-stage")?.getAttribute("aria-label") ?? null,
    menuOpen: document.querySelector(".pico-menu-list") !== null,
  }
})

async function idle(page: Page) {
  await page.clock.fastForward(PICO_ATTRACT_AFTER_MS + 1_000)
  await page.waitForTimeout(50)
}

/* The portal's spatial focus: focus moves, with no key event. */
const direction = (page: Page) => page.evaluate(() => {
  const carts = [...document.querySelectorAll<HTMLButtonElement>(".pico-cart-shelf-rack button")]
  const at = carts.indexOf(document.activeElement as HTMLButtonElement)
  carts[at + 1]?.focus()
})
/* The portal's confirm: a click on the focused control. */
const confirm = (page: Page) => page.evaluate(() => (document.activeElement as HTMLElement | null)?.click())
const focused = (page: Page) => page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? "")
const button = (page: Page, action: SurfaceInputAction) => page.evaluate(a => window.picoPress(a), action)

describe("waking from attract on two displays", () => {
  for (const [name, wake] of [
    ["confirm", confirm],
    ["back", (page: Page) => button(page, "back")],
    ["menu", (page: Page) => button(page, "menu")],
  ] as const) {
    run(`${name} brings the showcase back to the main display and ends attract on both`, async () => {
      const page = await open("rgds")
      await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-cart-shelf-rack button")?.focus())
      await idle(page)
      expect((await drawn(page)).attract).toEqual({ show: true, choose: true })
      await wake(page)
      const after = await drawn(page)
      expect(after.attract).toEqual({ show: false, choose: false })
      expect(after.showcase).toBe("Celeste Classic")
      await page.close()
    }, 30_000)
  }

  run("a direction ends attract on both, and only wakes: the cursor and the showcase stay", async () => {
    const page = await open("rgds")
    await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-cart-shelf-rack button")?.focus())
    await idle(page)
    expect((await drawn(page)).attract).toEqual({ show: true, choose: true })
    await direction(page)
    const after = await drawn(page)
    expect(after.attract).toEqual({ show: false, choose: false })
    expect(after.showcase).toBe("Celeste Classic")
    expect(await focused(page)).toContain("Celeste Classic")
    // The next direction moves the cursor as usual.
    await direction(page)
    expect((await drawn(page)).showcase).toBe("Hollow Knight")
    await page.close()
  }, 30_000)

  for (const region of ["show", "choose"] as const) {
    run(`a tap on the ${region === "show" ? "main" : "choosing"} display ends attract on both and only wakes`, async () => {
      const page = await open("rgds")
      await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-cart-shelf-rack button")?.focus())
      await idle(page)
      await page.mouse.click(320, region === "show" ? 240 : 720)
      const after = await drawn(page)
      expect(after.attract).toEqual({ show: false, choose: false })
      expect(after.showcase).toBe("Celeste Classic")
      expect(after.menuOpen).toBe(false)
      expect(await focused(page)).toContain("Celeste Classic")
      await page.close()
    }, 30_000)
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
    expect(after.showcase).toBe("Celeste Classic")
    await page.close()
  }, 30_000)
})

describe("waking from attract on one display", () => {
  run("back ends attract as before", async () => {
    const page = await open("none")
    await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-cart-shelf-rack button")?.focus())
    await idle(page)
    expect((await drawn(page)).attract.choose).toBe(true)
    await button(page, "back")
    expect((await drawn(page)).attract.choose).toBe(false)
    await page.close()
  }, 30_000)

  run("a direction ends attract and leaves the cursor where it was", async () => {
    const page = await open("none")
    await page.evaluate(() => document.querySelector<HTMLButtonElement>(".pico-cart-shelf-rack button")?.focus())
    await idle(page)
    expect((await drawn(page)).attract.choose).toBe(true)
    await direction(page)
    expect((await drawn(page)).attract.choose).toBe(false)
    expect(await focused(page)).toContain("Celeste Classic")
    await page.close()
  }, 30_000)
})
