#!/usr/bin/env node
/** Run against an already-started Caliper; never starts a kiosk or backend. */
import assert from "node:assert/strict"
import { readdirSync, readFileSync, writeFileSync, unlinkSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { chromium } from "playwright"
import { verifyMotion } from "./verify-motion.mjs"
import { verifyPhysical } from "./verify-physical.mjs"
import { verifyLivePages } from "./verify-pages.mjs"

const project = "korri-pico-a7960dd2"
const url = process.env.CALIPER_URL ?? "http://127.0.0.1:3131"
const sourceRoot = fileURLToPath(new URL("../src/", import.meta.url))
const expectedParts = readdirSync(sourceRoot, { recursive: true }).filter(p => p.endsWith(".part.tsx")).length
assert(expectedParts > 0)
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
})
try {
  const page = await browser.newPage({ viewport: { width: 1500, height: 950 }, reducedMotion: "reduce" })
  const errors = []
  page.on("pageerror", error => errors.push(error.message))
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); if (process.env.DEBUG_PICO) console.error(message.text().slice(0,350)) })
  await page.goto(url, { waitUntil: "domcontentloaded" })
  await page.locator("select").selectOption(project)
  const frame = page.frameLocator(`iframe[src*="${project}"]`)
  const scope = frame.locator('[aria-label^="Pico fixture:"]')
  await scope.first().waitFor()
  await frame.locator(".lab-scale-content").first().waitFor({ state: "attached" })
  // Discovery must render every authored part, not just report a ready session.
  await page.waitForFunction(({ project, count }) => {
    const doc = document.querySelector(`iframe[src*="${project}"]`)?.contentDocument
    return doc?.querySelectorAll(".lab-scale-content").length === count
  }, { project, count: expectedParts })
  while (await frame.locator(".pt-card.is-sel").count()) await frame.locator(".pt-card.is-sel").first().dispatchEvent("click")
  assert.equal(await scope.count(), 3, "three physical devices")
  const first = scope.first()
  await first.focus()
  await page.keyboard.press("f")
  await first.locator(".pico-library-browser").waitFor()
  assert.equal(await scope.nth(1).locator(".pico-library-browser").count(), 0, "keyboard must stay scoped")
  await page.keyboard.press("Escape")
  await first.locator(".pico-cart-shelf").waitFor()
  await page.keyboard.press("m")
  await first.locator(".pico-cart-grid").waitFor()
  await page.keyboard.press("m")
  await first.locator(".pico-game-hero").waitFor()
  await page.keyboard.press("s")
  await first.locator(".pico-panel-screen").waitFor()

  const failures = []
  // Placement is a different rendering path from gallery previews: Caliper
  // synthesizes a prop contract for the real component root.
  const cards = frame.locator(".pt-card")
  assert.equal(await cards.count(), expectedParts)
  while (await frame.locator(".pt-card.is-sel").count()) await frame.locator(".pt-card.is-sel").first().dispatchEvent("click")
  for (let index = 0; index < expectedParts; index++) {
    const card = cards.nth(index)
    await card.dispatchEvent("click")
    await page.waitForTimeout(70)
    const placementErrors = await frame.locator(".lab-preview-error").allTextContents()
    failures.push(...placementErrors)
    await card.dispatchEvent("click")
  }
  await verifyLivePages(page, frame)
  const locationCard = frame.getByRole("button", { name: "Open Location Picker", exact: true })
  await locationCard.dispatchEvent("click")
  const locationObject = frame.locator(".pt-object").filter({ has: frame.locator(".pico-location-picker") })
  await locationObject.dispatchEvent("pointerdown")
  await frame.getByRole("combobox", { name: "Preview frame device size", exact: true }).selectOption("rg353m")
  const fits = await locationObject.evaluate(object => {
    const bounds = object.querySelector(".lab-compose-screen-frame").getBoundingClientRect()
    const content = object.querySelector(".pico-caliper-preview").getBoundingClientRect()
    const buttons = [...object.querySelectorAll(".pico-location-picker button")]
    return buttons.length === 2 && Math.abs(content.width - bounds.width) < 2 &&
      Math.abs(content.height - bounds.height) < 2 && buttons.every(button => {
        const box = button.getBoundingClientRect()
        return box.left >= bounds.left && box.right <= bounds.right && box.top >= bounds.top && box.bottom <= bounds.bottom
      })
  })
  if (!fits) failures.push("placed Location Picker does not fit its RG353M frame")
  await locationCard.dispatchEvent("click")
  const badgeCard = frame.getByRole("button", { name: "Open Badge", exact: true })
  await badgeCard.dispatchEvent("click")
  const badgeObject = frame.locator(".pt-object").filter({ has: frame.locator(".pico-badge") })
  await badgeObject.dispatchEvent("pointerdown")
  assert.equal(await frame.getByRole("combobox", { name: "Data source for Badge", exact: true }).count(), 0,
    "static smaller parts must not advertise an inert source picker")
  await frame.getByRole("combobox", { name: "Tone for Badge", exact: true }).selectOption("string:warn")
  assert.equal(await badgeObject.locator(".pico-badge").getAttribute("data-tone"), "warn")
  assert.equal(await badgeObject.locator(".pico-badge").innerText(), "SAVING", "Inspector edit must retain the authored required text")
  await badgeCard.dispatchEvent("click")
  const sizes = await frame.locator(".lab-scale-content").evaluateAll(nodes => nodes.map(e => ({width: e.clientWidth, height: e.clientHeight})))
  if (sizes.some(size => size.width <= 0 || size.height <= 0)) failures.push("part preview collapsed to zero dimensions")
  const knob = frame.getByRole("spinbutton", { name: "--intrinsic-base-cqi value", exact: true })
  const original = await knob.inputValue()
  try {
    await knob.fill("3.5")
    await knob.press("Tab")
    await page.waitForTimeout(300)
    const value = await first.locator(".pico-theme").evaluate(e => getComputedStyle(e).getPropertyValue("--intrinsic-base-cqi").trim())
    if (Number(value) !== 3.5) failures.push(`design knob did not reach Pico: ${value}`)
  } finally {
    await knob.fill(original)
    await knob.press("Tab")
  }
  if (process.env.VERIFY_HMR === "1") {
    const path = `${sourceRoot}/CaliperHmrProbe${process.pid}.atom.part.tsx`
    const probe = name => `import { PicoBadge } from "./ui/atoms/PicoBadge"\nexport const name = "${name}"\nexport default function CaliperHmrProbe() { return <PicoBadge text="HMR" tone="info" /> }\n`
    const origin = await first.evaluate(() => performance.timeOrigin)
    writeFileSync(path, probe("HMR probe initial"), { flag: "wx" })
    try {
      await frame.getByRole("button", { name: "Open HMR probe initial", exact: true }).waitFor({ state: "attached" })
      assert.equal(await cards.count(), expectedParts + 1)
      writeFileSync(path, probe("HMR probe updated"))
      await frame.getByRole("button", { name: "Open HMR probe updated", exact: true }).waitFor({ state: "attached" })
    } finally { unlinkSync(path) }
    await frame.getByRole("button", { name: "Open HMR probe updated", exact: true }).waitFor({ state: "detached" })
    await page.waitForFunction(({ project, count }) =>
      document.querySelector(`iframe[src*="${project}"]`)?.contentDocument?.querySelectorAll(".pt-card").length === count,
      { project, count: expectedParts })
    assert.equal(await cards.count(), expectedParts)
    assert.equal(await first.evaluate(() => performance.timeOrigin), origin, "part add/edit/remove must not reload the session")
    const pagePath = `${sourceRoot}/pages/CaliperHmrPage${process.pid}.page.part.tsx`
    const original = readFileSync(`${sourceRoot}/pages/PicoSettings.page.part.tsx`, "utf8")
    writeFileSync(pagePath, original.replace('name = "Settings"', 'name = "HMR Settings initial"'), { flag: "wx" })
    try {
      const pageCard = frame.getByRole("button", { name: "Open HMR Settings initial", exact: true })
      await pageCard.waitFor({ state: "attached" })
      await pageCard.dispatchEvent("click")
      await page.waitForTimeout(100)
      assert.deepEqual(await frame.locator(".lab-preview-error").allTextContents(), [])
      const placed = frame.locator(".lab-part-mount .pico-panel-screen")
      await placed.waitFor()
      writeFileSync(pagePath, original.replace('name = "Settings"', 'name = "HMR Settings updated"'))
      const updated = frame.getByRole("button", { name: "Open HMR Settings updated", exact: true })
      await updated.waitFor({ state: "attached" })
      await placed.waitFor()
      await updated.dispatchEvent("click")
    } finally {
      for (const card of await frame.locator('.pt-card.is-sel[aria-label^="Open HMR Settings"]').all()) await card.dispatchEvent("click")
      unlinkSync(pagePath)
    }
    assert.equal(await first.evaluate(() => performance.timeOrigin), origin, "page HMR must not reload the session")
    console.log("PASS: source-part add/edit/remove HMR without reloading the session")
  }
  await verifyMotion(page, frame)
  if (process.env.PHYSICAL_REVIEW_DIR) await verifyPhysical(page, frame, process.env.PHYSICAL_REVIEW_DIR)
  failures.push(...errors)
  assert.deepEqual(failures, [])
  if (process.env.SCREENSHOT) await page.screenshot({ path: process.env.SCREENSHOT })
  console.log(`PASS: ${expectedParts} discovered parts, 3 devices, scoped navigation, all part placements and preview bounds, RG353M part resizing, Inspector prop edit, live design knob, no browser errors`)
} finally {
  await browser.close()
}
