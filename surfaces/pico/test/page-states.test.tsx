import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render, within } from "@testing-library/react"
import * as Home from "../src/pages/PicoHome.page.part"
import * as Detail from "../src/pages/PicoGameDetail.page.part"
import * as Library from "../src/pages/PicoLibrary.page.part"
import * as Overlay from "../src/pages/PicoOverlay.page.part"
import * as Runner from "../src/pages/PicoRunnerPicker.page.part"
import * as Settings from "../src/pages/PicoSettings.page.part"
import * as Panel from "../src/pages/PicoSettingsPanel.page.part"
import * as Shell from "../src/ui/templates/PicoScreenShell.template.part"
import * as PanelTemplate from "../src/ui/templates/PicoPanelScreen.template.part"
import * as OverlayTemplate from "../src/ui/templates/PicoGameOverlay.template.part"

afterEach(cleanup)

test("home catalog states state loading, emptiness and the read failure", () => {
  const loading = render(<Home.Loading />)
  expect(loading.getByText("READING CARTS")).toBeDefined()
  loading.unmount()
  const empty = render(<Home.Empty />)
  expect(empty.getByText("NO CARTS")).toBeDefined()
  empty.unmount()
  const failed = render(<Home.Error />)
  expect(failed.getByText("The library could not be read.")).toBeDefined()
  expect(failed.getByRole("button", { name: "TRY AGAIN" })).toBeDefined()
})

test("launch problems only offer retry when the model permits it", () => {
  const retryable = render(<Home.RetryableProblem />)
  expect(retryable.getByRole("button", { name: "TRY AGAIN" })).toBeDefined()
  retryable.unmount()
  const terminal = render(<Home.Problem />)
  expect(terminal.queryByRole("button", { name: "TRY AGAIN" })).toBeNull()
  expect(terminal.getByRole("button", { name: "OK" })).toBeDefined()
})

test("detail uses the real location choice and destructive confirmation", () => {
  const placing = render(<Detail.ChooseLocation />)
  expect(placing.getByRole("button", { name: /This device/ })).toBeDefined()
  expect(placing.getByRole("button", { name: /zao/ })).toBeDefined()
  placing.unmount()
  const confirming = render(<Detail.ConfirmRemoval />)
  expect(confirming.getByRole("dialog")).toBeDefined()
  expect(confirming.getByRole("button", { name: "REMOVE FROM DEVICE" })).toBeDefined()
})

test("search and empty results are derived from the catalog", () => {
  const results = render(<Library.SearchResults />)
  expect(results.getByRole("button", { name: /Hollow Knight/ })).toBeDefined()
  expect(results.queryByRole("button", { name: /Tetris/ })).toBeNull()
  results.unmount()
  const none = render(<Library.NoResults />)
  expect(none.getByText("NOTHING MATCHES")).toBeDefined()
  none.unmount()
  const empty = render(<Library.Empty />)
  expect(empty.getByText("NOTHING MATCHES")).toBeDefined()
})

test("overlay problems preserve retry policy and quit asks first", () => {
  const retry = render(<Overlay.RetryableProblem />)
  expect(retry.getByRole("button", { name: "TRY AGAIN" })).toBeDefined()
  retry.unmount()
  const terminal = render(<Overlay.Problem />)
  expect(terminal.queryByRole("button", { name: "TRY AGAIN" })).toBeNull()
  terminal.unmount()
  const confirm = render(<Overlay.ConfirmQuit />)
  expect(confirm.getByRole("button", { name: "QUIT GAME" })).toBeDefined()
})

test("runner loading and busy show no stale launch controls", () => {
  for (const State of [Runner.Loading, Runner.Busy]) {
    const view = render(<State />)
    expect(view.getByRole("button", { name: "Cancel" })).toBeDefined()
    expect(view.queryByRole("button", { name: "Launch once" })).toBeNull()
    view.unmount()
  }
  const ready = render(<Runner.Ready />)
  expect(ready.getAllByRole("button", { name: "Launch once" }).length).toBe(2)
})

test("runner conflict, error and warnings expose the host's distinct outcomes", () => {
  const conflict = render(<Runner.Conflict />)
  expect(conflict.getByRole("button", { name: "Stop active session" })).toBeDefined()
  conflict.unmount()
  const error = render(<Runner.Error />)
  expect(error.getByRole("button", { name: "Reload runners" })).toBeDefined()
  error.unmount()
  const warnings = render(<Runner.Warnings />)
  expect(warnings.getByText("Launched with settings warnings")).toBeDefined()
  expect(warnings.getByRole("button", { name: "Done" })).toBeDefined()
})

test("settings and panel publish real empty, saving and failure rows", () => {
  for (const State of [Settings.Empty, Panel.Empty]) {
    const view = render(<State />)
    expect(view.getByText("NOTHING TO SET")).toBeDefined()
    view.unmount()
  }
  for (const State of [Settings.Saving, Panel.Saving]) {
    const view = render(<State />)
    expect(view.getByText("SAVING")).toBeDefined()
    view.unmount()
  }
  for (const State of [Settings.Error, Panel.Error]) {
    const view = render(<State />)
    expect(view.getByText("The device name could not be saved.")).toBeDefined()
    expect(view.getByRole("button", { name: "OK" })).toBeDefined()
    view.unmount()
  }
})

test("settings confirmation reuses the fixture copy; text settings open the editor", () => {
  const confirming = render(<Settings.ConfirmForget />)
  expect(confirming.getByRole("dialog", { name: "FORGET EVERYTHING?" })).toBeDefined()
  confirming.unmount()
  const name = render(<Settings.EditName />)
  expect(name.getByRole("textbox", { name: "Name" }).textContent).toContain("usu")
  expect(name.getByRole("button", { name: "Capitals" })).toBeDefined()
  name.unmount()
  const secret = render(<Settings.EditSecret />)
  expect(secret.getByRole("button", { name: "CLEAR SAVED KEY" })).toBeDefined()
  secret.unmount()
  const clearing = render(<Settings.ConfirmClearSecret />)
  expect(clearing.getByRole("dialog", { name: "CLEAR SAVED KEY?" })).toBeDefined()
})

test("template variants fill their own slots without changing the templates", () => {
  const shell = render(<Shell.WithoutClockOrReadout />)
  expect(shell.queryByText("10:24")).toBeNull()
  expect(within(shell.getByRole("main")).getByText("READING CARTS")).toBeDefined()
  shell.unmount()
  const panel = render(<PanelTemplate.SecondCategory />)
  expect(panel.getByRole("region", { name: "PLUGINS" })).toBeDefined()
  expect(panel.getByText("mGBA")).toBeDefined()
  panel.unmount()
  const overlay = render(<OverlayTemplate.PauseMenu />)
  expect(overlay.getByRole("dialog", { name: "Hollow Knight" })).toBeDefined()
  expect(overlay.getByRole("button", { name: /Continue playing/ })).toBeDefined()
})
