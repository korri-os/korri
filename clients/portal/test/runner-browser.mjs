import assert from "node:assert/strict"
import { mkdir } from "node:fs/promises"
import { createRequire } from "node:module"
const { chromium } = createRequire(new URL("../../../surfaces/pico/package.json", import.meta.url))(
  "playwright",
)

const url = process.env.RUNNER_PREVIEW_URL ?? "http://127.0.0.1:5197/test/runner-preview.html"
const output = process.env.RUNNER_SCREENSHOTS ?? "/tmp/korri-runner-screenshots"
await mkdir(output, { recursive: true })
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
})
const page = await browser.newPage({
  viewport: { width: 1500, height: 1000 },
  reducedMotion: "reduce",
})
const errors = []
page.on("pageerror", error => errors.push(error.message))
page.on("response", response => {
  if (response.status() >= 400 && /\.(woff2?|css|tsx?|js)(\?|$)/.test(response.url()))
    errors.push(`${response.status()} ${response.url()}`)
})
page.on("request", request => {
  const parsed = new URL(request.url())
  assert(
    parsed.origin === new URL(url).origin || parsed.protocol === "data:",
    `No live network in fixture: ${request.url()}`,
  )
  assert.notEqual(parsed.pathname, "/rpc")
})
const dialog = () => page.getByRole("dialog", { name: "Runners for Wario Land 4" })
async function load(surface, fixture = "stale") {
  await page.goto(`${url}?surface=${surface}&fixture=${fixture}`)
  await page
    .getByRole("button", { name: /Wario Land 4/ })
    .first()
    .waitFor()
  await page.evaluate(() => document.fonts.ready)
}
async function open(surface) {
  if (surface === "pico") {
    const cart = page.getByRole("button", { name: "Wario Land 4, This device", exact: true })
    await cart.click()
    if (!(await page.getByRole("button", { name: "▶ PLAY", exact: true }).count()))
      await cart.click()
    await page.getByRole("button", { name: "▶ PLAY", exact: true }).click()
  } else {
    await page.getByRole("button", { name: "Wario Land 4", exact: true }).click()
  }
  await dialog().waitFor()
}
try {
  for (const surface of ["shift", "pico"]) {
    await load(surface)
    await open(surface)
    assert.equal(
      await dialog().getByRole("button", { name: "Launch once", exact: true }).count(),
      2,
    )
    assert((await dialog().innerText()).includes("missing/runner"))
    for (const [name, width, height] of [
      ["generous", 1280, 800],
      ["medium", 640, 480],
      ["narrow-tall", 360, 720],
      ["wide-short", 960, 220],
      ["tiny", 240, 180],
    ]) {
      await page.locator("#app").evaluate(
        (element, [width, height]) => {
          element.style.setProperty("--fixture-width", `${width}px`)
          element.style.setProperty("--fixture-height", `${height}px`)
        },
        [width, height],
      )
      // Every action must be focusable and scroll into the actual container.
      for (const button of await dialog().getByRole("button").all()) {
        await button.focus()
        await button.scrollIntoViewIfNeeded()
        const box = await button.boundingBox()
        assert(
          box &&
            box.x >= -1 &&
            box.y >= -1 &&
            box.x + box.width <= width + 1 &&
            box.y + box.height <= height + 1,
          `${surface} ${name}: clipped ${await button.innerText()} ${JSON.stringify(box)}`,
        )
      }
      await dialog().getByRole("button", { name: "Launch once", exact: true }).first().focus()
      for (let step = 0; step < (await dialog().getByRole("button").count()); step++) {
        if (
          await dialog()
            .getByRole("button", { name: "Cancel", exact: true })
            .evaluate(element => element === document.activeElement)
        )
          break
        await page.keyboard.press("ArrowDown")
      }
      assert(
        await dialog()
          .getByRole("button", { name: "Cancel", exact: true })
          .evaluate(element => element === document.activeElement),
        `${surface} ${name}: directional input reaches the final action`,
      )
      await dialog().getByRole("button", { name: "Launch once", exact: true }).first().focus()
      await page.locator("#app").screenshot({ path: `${output}/${surface}-${name}.png` })
      await page.keyboard.press("ArrowDown")
      assert(
        await dialog().evaluate(element => element.contains(document.activeElement)),
        "directional focus must remain in picker",
      )
    }
    await page.keyboard.press("Escape")
    await dialog().waitFor({ state: "detached" })
    if (surface === "pico")
      assert.equal(
        await page.getByRole("button", { name: "▶ PLAY", exact: true }).count(),
        1,
        "Back must not also leave game detail",
      )
    assert(
      await page
        .getByRole("button", { name: surface === "pico" ? "▶ PLAY" : "Wario Land 4", exact: true })
        .evaluate(element => element === document.activeElement),
      "Cancel restores the opener's focus",
    )

    // Saved choices remain reachable without playing, including after saving.
    await load(surface)
    if (surface === "pico")
      await page.getByRole("button", { name: "Wario Land 4, This device", exact: true }).click()
    else {
      await page.getByRole("button", { name: "Wario Land 4", exact: true }).focus()
      await page.keyboard.press("o")
    }
    await page.getByRole("button", { name: "Runners on this device", exact: true }).click()
    await dialog()
      .getByRole("button", { name: "Remember for this game", exact: true })
      .nth(1)
      .click()
    await dialog().getByText("This game: retroarch/mgba-nightly", { exact: true }).waitFor()
    await dialog().getByRole("button", { name: "Clear game choice", exact: true }).click()
    await dialog()
      .getByRole("button", { name: "Clear game choice", exact: true })
      .waitFor({ state: "detached" })
    await dialog()
      .getByRole("button", { name: "Remember for system gba", exact: true })
      .nth(1)
      .click()
    await dialog().getByText("System gba: retroarch/mgba-nightly", { exact: true }).waitFor()
    await dialog().getByRole("button", { name: "Clear system gba choice", exact: true }).click()
    await dialog()
      .getByRole("button", { name: "Clear system gba choice", exact: true })
      .waitFor({ state: "detached" })
    await dialog().getByRole("button", { name: "Cancel", exact: true }).click()

    await load(surface, "conflict")
    await open(surface)
    await dialog().getByRole("button", { name: "Launch once", exact: true }).first().click()
    await dialog().getByRole("button", { name: "Stop active session", exact: true }).click()
    await dialog().getByRole("button", { name: "Launch once", exact: true }).first().waitFor()
    assert(
      (await dialog().innerText()).includes("missing/runner"),
      "stop returns to choice without launching or saving",
    )
    await dialog().getByRole("button", { name: "Launch once", exact: true }).nth(1).click()
    await dialog().waitFor({ state: "detached" })

    await load(surface, "warnings")
    await open(surface)
    await dialog().getByText("Launched with settings warnings", { exact: true }).waitFor()
    assert((await dialog().innerText()).includes("Unsupported in pinned version 1.20"))
    await page.locator("#app").screenshot({ path: `${output}/${surface}-warnings.png` })
    await dialog().getByRole("button", { name: "Done", exact: true }).click()
    await dialog().waitFor({ state: "detached" })

    await load(surface, "read-only")
    await open(surface)
    await dialog().getByRole("button", { name: "Launch once", exact: true }).first().click()
    await dialog().getByText("Launching requires session access", { exact: true }).waitFor()
    await dialog().getByRole("button", { name: "Reload runners", exact: true }).click()
    await dialog()
      .getByRole("button", { name: "Remember for this game", exact: true })
      .first()
      .click()
    await dialog()
      .getByText("Saving runner choices requires Full access", { exact: true })
      .waitFor()
    await page.keyboard.press("Escape")

    await load(surface, "loading")
    await open(surface)
    await page.keyboard.press("Escape")
    await page.waitForTimeout(4200)
    assert.equal(await dialog().count(), 0, "a cancelled read must not reopen chooser")
    assert.equal(errors.length, 0, errors.join("\n"))
  }
  console.log(`Verified both surfaces at five container sizes. Screenshots: ${output}`)
} finally {
  await browser.close()
}
