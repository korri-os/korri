import assert from "node:assert/strict"

export async function verifyMotion(page, frame) {
  const card = frame.getByRole("button", { name: "Open Attract", exact: true })
  await card.dispatchEvent("click")
  const object = frame.locator(".pt-object").filter({ has: frame.locator(".lab-part-mount .pico-attract") })
  await object.dispatchEvent("pointerdown")
  await frame.getByRole("combobox", { name: "Preview frame device size", exact: true }).selectOption("rg353m")
  const rail = object.locator(".pico-attract-rail")
  const geometry = await rail.evaluate(rail => {
    const cards = [...rail.querySelectorAll(".pico-attract-cart")]
    const period = cards[cards.length / 2].getBoundingClientRect().left - cards[0].getBoundingClientRect().left
    return { period, half: rail.getBoundingClientRect().width / 2, frame: rail.closest(".pico-screen").clientWidth }
  })
  assert(Math.abs(geometry.period - geometry.half) < 1 && geometry.period >= geometry.frame - 1,
    `Attract needs a complete repeating viewport at the seam: ${JSON.stringify(geometry)}`)
  await page.emulateMedia({ reducedMotion: "no-preference" })
  const motion = await rail.evaluate(rail => {
    const animation = rail.getAnimations().find(a => a.animationName === "pico-attract-drift")
    if (!animation) return null
    animation.pause()
    animation.currentTime = 0
    const start = getComputedStyle(rail).transform
    animation.currentTime = Number(animation.effect.getTiming().duration) / 2
    return { start, middle: getComputedStyle(rail).transform, easing: getComputedStyle(rail).animationTimingFunction }
  })
  assert(motion && motion.start !== motion.middle && motion.easing.startsWith("steps("), "Attract must actually move in discrete steps")
  await page.emulateMedia({ reducedMotion: "reduce" })
  assert.equal(await rail.evaluate(e => getComputedStyle(e).animationName), "none")
  await card.dispatchEvent("click")

  const homeCard = frame.locator(".pt-gallery-group").filter({ has: frame.locator(".pt-gallery-head .layer-page") })
    .getByRole("button", { name: "Open Home", exact: true })
  await page.clock.install()
  await homeCard.dispatchEvent("click")
  const home = frame.locator(".pt-object").filter({ has: frame.locator(".lab-part-mount .pico-caliper-mount") })
  await home.dispatchEvent("pointerdown")
  const cart = home.locator(".pico-cart").first()
  await cart.focus()
  await page.clock.fastForward(46_000)
  await home.getByRole("img", { name: "Attract", exact: true }).waitFor()
  // Real pointer events over the cart's covered position, not synthetic clicks.
  const box = await cart.boundingBox()
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2)
  assert.equal(await home.locator(".pico-attract").count(), 0)
  assert.equal(await home.locator(".pico-game-detail").count(), 0)
  await cart.click()
  await home.locator(".pico-game-detail").waitFor()
  await homeCard.dispatchEvent("click")
  console.log("PASS: attract seam, stepped motion, reduced motion and real pointer wake without activation")
}
