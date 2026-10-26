/**
 * Two displays in a real browser, through a window that grows after Pico
 * mounts, as the RG DS kiosk does: 640 × 480 on the main display first, then
 * 640 × 960 across both.
 *
 * happy-dom does no layout and no focus scrolling, so this one runs in
 * Chromium. Set CHROMIUM to a Chromium executable; without it the test is
 * skipped, and says so. Vite serves the page from its own process, because
 * the happy-dom preload replaces the timers Vite needs in this one.
 */
import { afterAll, beforeAll, expect, test } from "bun:test"
import { join } from "node:path"
import { type Browser, chromium } from "playwright-core"

const executablePath = process.env.CHROMIUM
const run = executablePath === undefined || executablePath === "" ? test.skip : test

const PICO = join(import.meta.dir, "..")
let server: ReturnType<typeof Bun.spawn> | undefined
let address: string | undefined
let browser: Browser | undefined

/** Start Vite on Pico's source and read the address it prints. */
async function serve(): Promise<string> {
  const vite = Bun.spawn(
    [process.execPath, join(PICO, "node_modules/vite/bin/vite.js"), "--config", "test/browser/vite.config.ts", "--port", "0"],
    { cwd: PICO, stdout: "pipe", stderr: "inherit", env: { ...process.env, NO_COLOR: "1" } },
  )
  server = vite
  const reader = (vite.stdout as ReadableStream<Uint8Array>).getReader()
  let printed = ""
  for (;;) {
    const { value, done } = await reader.read()
    if (done) throw new Error(`vite stopped before it served: ${printed}`)
    printed += new TextDecoder().decode(value)
    const found = printed.match(/http:\/\/127\.0\.0\.1:\d+\//)
    if (found !== null) return found[0]
  }
}

beforeAll(async () => {
  if (executablePath === undefined || executablePath === "") return
  address = await serve()
  browser = await chromium.launch({ executablePath })
}, 30_000)

afterAll(async () => {
  await browser?.close()
  server?.kill()
})

interface Placement {
  readonly show: readonly number[]
  readonly choose: readonly number[]
  /** Every element the browser scrolled, and how far. */
  readonly scrolled: readonly string[]
  readonly focused: string | null
}

run("a window that grows after mount keeps each display on its own glass", async () => {
  if (browser === undefined || address === undefined) throw new Error("no browser or server")
  const page = await browser.newPage({ viewport: { width: 640, height: 480 } })
  await page.goto(new URL("test/browser/displays-resize.html", address).href)
  await page.waitForFunction(() => typeof window.picoStep === "function")

  const placement = () => page.evaluate((): Placement => {
    const rect = (selector: string) => {
      const box = document.querySelector(selector)?.getBoundingClientRect()
      return box === undefined ? [] : [box.x, box.y, box.width, box.height].map(Math.round)
    }
    return {
      show: rect("[data-pico-display='show']"),
      choose: rect("[data-pico-display='choose']"),
      scrolled: [document.documentElement, ...document.querySelectorAll("*")]
        .filter(element => element.scrollTop !== 0)
        .map(element => `${element.tagName.toLowerCase()}.${element.className}: ${element.scrollTop}`),
      focused: document.activeElement?.getAttribute("aria-label") ?? null,
    }
  })

  await page.evaluate(() => window.picoStep("displays"))
  await page.evaluate(() => window.picoStep("ready"))
  await page.waitForSelector("[data-pico-display='choose'] .pico-cart-shelf-rack button")
  // The host's first press puts the cursor on a cart, at the window's old size.
  await page.keyboard.press("Tab")
  const small = await placement()
  expect(small.focused ?? "").toContain("Celeste Classic")
  expect(small.scrolled).toEqual([])
  expect(small.show).toEqual([0, 0, 640, 480])
  expect(small.choose).toEqual([0, 480, 640, 480])

  await page.setViewportSize({ width: 640, height: 960 })
  await page.waitForFunction(() => window.innerHeight === 960)
  const grown = await placement()
  expect(grown.scrolled).toEqual([])
  expect(grown.show).toEqual([0, 0, 640, 480])
  expect(grown.choose).toEqual([0, 480, 640, 480])
  await page.close()
}, 30_000)
