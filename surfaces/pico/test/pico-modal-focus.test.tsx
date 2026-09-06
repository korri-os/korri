import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render } from "@testing-library/react"
import { PicoModal } from "../src/ui/organisms/PicoModal"

afterEach(cleanup)

test("a confirmation owns keyboard focus and restores it to its opener", () => {
  let cancelled = 0
  const opener = <button>OPEN</button>
  const view = render(<div className="pico-screen">{opener}</div>)
  view.getByRole("button", { name: "OPEN" }).focus()
  view.rerender(<div className="pico-screen">{opener}<PicoModal title="Confirm" message="Really?"
    confirmLabel="REMOVE" onConfirm={() => {}} onCancel={() => { cancelled++ }} /></div>)
  const cancel = view.getByRole("button", { name: "CANCEL" })
  const confirm = view.getByRole("button", { name: "REMOVE" })
  expect(document.activeElement === cancel).toBe(true)
  fireEvent.keyDown(cancel, { key: "Tab" })
  expect(document.activeElement === confirm).toBe(true)
  fireEvent.keyDown(confirm, { key: "Tab", shiftKey: true })
  expect(document.activeElement === cancel).toBe(true)
  fireEvent.keyDown(cancel, { key: "Escape" })
  expect(cancelled).toBe(1)
  view.rerender(<div className="pico-screen">{opener}</div>)
  expect(document.activeElement === view.getByRole("button", { name: "OPEN" })).toBe(true)
})

test("independent preview dialogs have distinct accessible titles without stealing focus", () => {
  const view = render(<>
    <button>INSPECTOR</button>
    <div className="pico-screen"><PicoModal title="One" message="First" confirmLabel="YES"
      onConfirm={() => {}} onCancel={() => {}} /></div>
  </>)
  view.getByRole("button", { name: "INSPECTOR" }).focus()
  view.rerender(<>
    <button>INSPECTOR</button>
    <div className="pico-screen"><PicoModal title="One" message="First" confirmLabel="YES"
      onConfirm={() => {}} onCancel={() => {}} /></div>
    <div className="pico-screen"><PicoModal title="Two" message="Second" confirmLabel="YES"
      onConfirm={() => {}} onCancel={() => {}} /></div>
  </>)
  const labels = view.getAllByRole("dialog").map(dialog => dialog.getAttribute("aria-labelledby"))
  expect(new Set(labels).size).toBe(2)
  expect(view.getByRole("dialog", { name: "Two" }).textContent).toContain("Second")
  expect(document.activeElement === view.getByRole("button", { name: "INSPECTOR" })).toBe(true)
})
