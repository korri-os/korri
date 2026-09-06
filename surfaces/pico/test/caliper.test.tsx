import { afterEach, expect, test } from "bun:test"
import { act, within } from "@testing-library/react"
import { readFileSync } from "node:fs"
import { picoAdapter } from "../caliper/adapter"

const mounted: { dispose(): void }[] = []
afterEach(async () => {
  await act(async () => {
    mounted.splice(0).forEach(instance => instance.dispose())
    await Promise.resolve()
  })
  document.body.replaceChildren()
})

function host() {
  const node = document.createElement("div")
  document.body.append(node)
  return node
}

const adapter = picoAdapter

test("the launcher entry exports the discovered parts bridge", () => {
  expect(readFileSync(new URL("../project-entry.ts", import.meta.url), "utf8"))
    .toContain('export { partsGlob } from "../pico-caliper-parts"')
})

test("integration files are included in typechecking", () => {
  const config = JSON.parse(readFileSync(new URL("../tsconfig.json", import.meta.url), "utf8"))
  expect(config.include).toContain("caliper")
  expect(config.include).toContain("project-entry.ts")
})

test("scoped Caliper events reach only their mounted Pico", async () => {
  const first = host(), second = host()
  await act(async () => {
    mounted.push(adapter.mountSurface(first, { mode: "fixture", scopeId: "one" }))
    mounted.push(adapter.mountSurface(second, { mode: "fixture", scopeId: "two" }))
  })
  await act(async () => {
    adapter.eventsForScreen("/")[0]!.emit("options", { scopeId: "one" })
  })
  expect(first.querySelector(".pico-library-browser")).not.toBeNull()
  expect(second.querySelector(".pico-library-browser")).toBeNull()
})

test("keyboard shortcuts are scoped, ignore typing and detach on dispose", async () => {
  const node = host()
  let instance: { dispose(): void }
  await act(async () => {
    instance = adapter.mountSurface(node, { mode: "fixture" })
    mounted.push(instance)
  })
  const input = document.createElement("input")
  node.append(input)
  await act(async () => {
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "f", bubbles: true }))
  })
  expect(node.querySelector(".pico-library-browser")).toBeNull()
  await act(async () => {
    node.dispatchEvent(new KeyboardEvent("keydown", { key: "f", bubbles: true }))
  })
  expect(node.querySelector(".pico-library-browser")).not.toBeNull()
  await act(async () => { instance!.dispose(); await Promise.resolve() })
  const key = new KeyboardEvent("keydown", { key: "f", bubbles: true, cancelable: true })
  node.dispatchEvent(key)
  expect(key.defaultPrevented).toBe(false)
})

test("fixture launch publishes its consequence without invoking a backend", async () => {
  const node = host()
  await act(async () => { mounted.push(adapter.mountSurface(node, { mode: "fixture" })) })
  await act(async () => { (node.querySelector(".pico-cart") as HTMLElement).click() })
  const play = within(node).getByRole("button", { name: /play/i })
  await act(async () => { play.click() })
  expect(node.querySelector(".pico-launch-stage")).not.toBeNull()
  expect(node.textContent).toContain("PREVIEW")
})

test("settings and overlay fixture interactions publish new values", async () => {
  const node = host()
  await act(async () => { mounted.push(adapter.mountSurface(node, { mode: "fixture", scopeId: "feedback" })) })
  await act(async () => { adapter.eventsForScreen("/")[0]!.emit("system", { scopeId: "feedback" }) })
  await act(async () => { within(node).getByRole("tab", { name: "PLUGINS" }).click() })
  await act(async () => { within(node).getByRole("button", { name: /mGBA/ }).click() })
  expect(within(node).getByRole("button", { name: /mGBA/ }).querySelector('[data-lit="true"]')?.textContent).toBe("OFF")
  await act(async () => { adapter.eventsForScreen("/")[1]!.emit("overlay", { scopeId: "feedback" }) })
  await act(async () => { within(node).getByRole("button", { name: /Fast forward/ }).click() })
  expect(within(node).getByRole("button", { name: /Fast forward/ }).textContent).toContain("ON")
  await act(async () => { within(node).getByRole("button", { name: /Shader/ }).click() })
  expect(within(node).getByRole("button", { name: /Shader/ }).textContent).toContain("CRT")
  await act(async () => { within(node).getByRole("button", { name: /Volume/ }).click() })
  expect(within(node).getByRole("button", { name: /Volume/ }).textContent).toContain("90")
})

test("deferred disposal cannot remove a replacement mounted in the same host", async () => {
  const node = host()
  await act(async () => {
    const old = adapter.mountSurface(node, { mode: "fixture", scopeId: "reused" })
    old.dispose()
    old.dispose()
    mounted.push(adapter.mountSurface(node, { mode: "fixture", scopeId: "reused" }))
    await Promise.resolve()
  })
  expect(node.querySelectorAll(".pico-caliper-mount").length).toBe(1)
  await act(async () => { adapter.eventsForScreen("/")[0]!.emit("options", { scopeId: "reused" }) })
  expect(node.querySelector(".pico-library-browser")).not.toBeNull()
})

test("the adapter refuses live-backend mode", () => {
  expect(() => adapter.mountSurface(host(), { mode: "live" })).toThrow("supports fixtures only")
})

test("overlay command previews acknowledge requests and quit returns to browsing", async () => {
  const node = host()
  await act(async () => { mounted.push(adapter.mountSurface(node, { mode: "fixture", initialValues: "overlay" })) })
  await act(async () => { within(node).getByRole("button", { name: "Save state" }).click() })
  expect(node.textContent).toContain("PREVIEW: request recorded")
  await act(async () => { within(node).getByRole("button", { name: /Quit game/ }).click() })
  await act(async () => { within(node).getByRole("button", { name: "QUIT GAME" }).click() })
  expect(node.querySelector(".pico-cart-shelf")).not.toBeNull()
})
