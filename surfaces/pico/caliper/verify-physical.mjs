import assert from "node:assert/strict"
import { mkdirSync, writeFileSync } from "node:fs"
import { join } from "node:path"

/** Capture the real placed parts at each configured panel size. Focus checks
 * distinguish deliberate shelf cropping from an unreachable action. */
export async function verifyPhysical(page, frame, directory) {
  mkdirSync(directory, { recursive: true })
  const cards = frame.locator(".pt-card")
  const results = []
  for (let index = 0; index < await cards.count(); index++) {
    const card = cards.nth(index)
    const name = await card.getAttribute("aria-label")
    await card.dispatchEvent("click")
    const object = frame.locator(".pt-object").filter({ has: frame.locator(".lab-part-mount") })
    await object.dispatchEvent("pointerdown")
    const picker = frame.getByRole("combobox", { name: "Preview frame device size", exact: true })
    const devices = await picker.locator("option").evaluateAll(options => options.map(o => o.value).filter(Boolean))
    assert.equal(devices.length, 3)
    for (const device of devices) {
      await picker.selectOption(device)
      await page.waitForTimeout(90)
      const screen = object.locator(".lab-compose-screen-frame")
      const file = `${String(index).padStart(2, "0")}-${device}.png`
      await screen.screenshot({ path: join(directory, file), animations: "disabled",
        style: ".pt-panel, .lab-object-dock { visibility: hidden !important; }" })
      const controls = object.locator(".lab-part-mount button:not([disabled]), .lab-part-mount input, .lab-part-mount select")
      const inaccessible = []
      for (let at = 0; at < await controls.count(); at++) {
        const control = controls.nth(at)
        await control.focus()
        await page.waitForTimeout(10)
        const result = await control.evaluate(element => {
          const frame = element.closest(".lab-compose-screen-frame").getBoundingClientRect()
          const box = element.getBoundingClientRect()
          const label = element.getAttribute("aria-label") ?? element.textContent?.trim() ?? ""
          return { label, inside: box.width > 0 && box.height > 0 &&
            box.left >= frame.left - 1 && box.right <= frame.right + 1 &&
            box.top >= frame.top - 1 && box.bottom <= frame.bottom + 1 }
        })
        if (!result.label || !result.inside) inaccessible.push(result)
      }
      results.push({ name, device, file, inaccessible })
    }
    await card.dispatchEvent("click")
  }
  writeFileSync(join(directory, "report.json"), JSON.stringify(results, null, 2))
  const failures = results.filter(r => r.inaccessible.length)
  console.log(`Physical review: ${results.length} captures; ${failures.length} part/device cases need focus-bound review. Evidence: ${directory}`)
  // Record the complete matrix before failing, so no first failure hides others.
  assert.deepEqual(failures.map(({ name, device, inaccessible }) => ({ name, device, inaccessible })), [])
}
