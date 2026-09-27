import { afterEach, expect, test } from "bun:test"
import { cleanup, fireEvent, render, within } from "@testing-library/react"
import * as attract from "../src/ui/organisms/PicoAttract.organism.part"
import * as grid from "../src/ui/organisms/PicoCartGrid.organism.part"
import * as shelf from "../src/ui/organisms/PicoCartShelf.organism.part"
import * as actions from "../src/ui/organisms/PicoGameActions.organism.part"
import * as detail from "../src/ui/organisms/PicoGameDetail.organism.part"
import * as hero from "../src/ui/organisms/PicoGameHero.organism.part"
import * as identity from "../src/ui/organisms/PicoIdentityDialog.organism.part"
import * as launch from "../src/ui/organisms/PicoLaunchStage.organism.part"
import * as library from "../src/ui/organisms/PicoLibraryBrowser.organism.part"
import * as locations from "../src/ui/organisms/PicoLocationPicker.organism.part"
import * as pause from "../src/ui/organisms/PicoPauseMenu.organism.part"
import * as modal from "../src/ui/organisms/PicoModal.organism.part"
import * as stage from "../src/ui/organisms/PicoGameStage.organism.part"
import { backupText, longTitleGame } from "../src/fixtures/named-states"

afterEach(cleanup)

test("empty attract retains the wake hint but not cartridges", () => {
  const view = render(<attract.EmptyLibrary />)
  expect(view.getByText("PRESS ANY BUTTON")).toBeDefined()
  expect(view.container.querySelectorAll(".pico-attract-cart").length).toBe(0)
})

test("ungrouped grid uses the product fallback caption and the long game title", () => {
  const view = render(<grid.UngroupedGames />)
  expect(view.getByRole("heading", { name: "GAMES" })).toBeDefined()
  expect(view.getByRole("button", { name: `${longTitleGame.title}, ${longTitleGame.subtitle}` })).toBeDefined()
})

test("resumable shelf starts on Hollow Knight and focus selects the next game", () => {
  const view = render(<shelf.ResumableFirst />)
  expect(view.getByRole("heading", { name: "Hollow Knight" })).toBeDefined()
  expect(view.getByText("RESUME")).toBeDefined()
  const buttons = within(view.getByRole("list", { name: "Shelf" })).getAllByRole("button")
  fireEvent.focus(buttons[1]!)
  expect(view.getByRole("heading", { name: longTitleGame.title })).toBeDefined()
  expect(view.queryByText("RESUME")).toBeNull()
})

test("disabled action retains the host's explanation and cannot be activated", () => {
  const view = render(<actions.DisabledAction />)
  expect(view.getByRole("button").hasAttribute("disabled")).toBe(true)
  expect(view.getByText("No card inserted")).toBeDefined()
})

test("never-played detail has PLAY, no actions, and no resume claim", () => {
  const view = render(<detail.NeverPlayedNoArtwork />)
  expect(view.getByRole("button", { name: "▶ PLAY" })).toBeDefined()
  expect(view.queryByText("RESUME")).toBeNull()
  expect(view.queryByRole("region", { name: "Actions" })).toBeNull()
  expect(view.container.querySelector(".pico-cover-art-initials")?.textContent).toBe("PQ")
  expect(view.container.querySelector("canvas")).toBeNull()
})

test("stage with no art shows initials and no invented resume or play facts", () => {
  const view = render(<stage.NoArtwork />)
  expect(view.getByRole("heading", { name: "Petal Quest" })).toBeDefined()
  expect(view.container.querySelector(".pico-cover-art-initials")?.textContent).toBe("PQ")
  expect(view.queryByText("RESUME")).toBeNull()
  expect(view.queryByText("PLAYS")).toBeNull()
})

test("long confirmation keeps both decisions reachable as controls", () => {
  const view = render(<modal.LongMessage />)
  const dialog = view.getByRole("dialog", { name: "FORGET EVERYTHING ON THIS DEVICE?" })
  expect(within(dialog).getByRole("button", { name: "FORGET" })).toBeDefined()
  expect(within(dialog).getByRole("button", { name: "CANCEL" })).toBeDefined()
  expect(dialog.textContent).toContain("Without a backup, the identity is unrecoverable after device loss.")
})

test("hero without play history does not invent LAST PLAYED", () => {
  const view = render(<hero.NoHistoryNoArtwork />)
  expect(view.queryByText("LAST PLAYED")).toBeNull()
  expect(view.getByRole("button", { name: "OPEN" })).toBeDefined()
})

test("unknown launch has no stand-in cartridge", () => {
  const view = render(<launch.UnknownGame />)
  expect(view.container.querySelector(".pico-launch-stage-cart")).toBeNull()
  expect(view.getByRole("heading", { name: "STARTING" })).toBeDefined()
})

test("running launch names the known game and uses the running phase", () => {
  const view = render(<launch.Running />)
  expect(view.container.querySelector('[data-phase="running"]')).not.toBeNull()
  expect(view.getByText("Hollow Knight")).toBeDefined()
})

test("no-result search keeps the query and recovery controls", () => {
  const view = render(<library.NoResults />)
  expect(view.getByText("NOTHING MATCHES")).toBeDefined()
  expect(view.container.textContent).toContain("ZZZZ")
  expect(view.getAllByRole("button").length).toBeGreaterThan(26)
})

test("filtered library marks the actual section and order", () => {
  const view = render(<library.CollectionByTitle />)
  expect(view.getByRole("button", { name: "THIS DEVICE" }).getAttribute("aria-pressed")).toBe("true")
  expect(view.getByRole("button", { name: "A-Z" }).getAttribute("aria-pressed")).toBe("true")
  expect(view.queryByText("Hollow Knight")).toBeNull()
  expect(view.getByText("Petal Quest")).toBeDefined()
})

test("location question retains the real two routes with a long title", () => {
  const view = render(<locations.LongTitle />)
  expect(view.getByRole("heading", { name: longTitleGame.title })).toBeDefined()
  expect(view.getByRole("button", { name: "This device" })).toBeDefined()
  expect(view.getByRole("button", { name: "zao" })).toBeDefined()
})

test("retry is offered only when the host allows it", () => {
  const view = render(<pause.RetryableProblem />)
  expect(view.getByRole("button", { name: "TRY AGAIN" })).toBeDefined()
  expect(view.getByText("zao stopped answering.")).toBeDefined()
  view.rerender(<pause.NonRetryableProblem />)
  expect(view.queryByRole("button", { name: "TRY AGAIN" })).toBeNull()
  expect(view.getByText("zao stopped answering.")).toBeDefined()
})

test("no-plugin pause menu retains core controls without the plugin group", () => {
  const view = render(<pause.NoPluginControls />)
  expect(view.getByRole("button", { name: /Continue playing/ })).toBeDefined()
  expect(view.queryByRole("region", { name: "MGBA" })).toBeNull()
})

test("identity work disables submission even with a password", () => {
  const view = render(<identity.Working />)
  fireEvent.change(view.getByLabelText("Password"), { target: { value: "preview-only-password" } })
  expect(view.getByRole("button", { name: "Encrypting backup" }).hasAttribute("disabled")).toBe(true)
})

test("backup-ready is inert preview text, not a live credential", () => {
  const view = render(<identity.BackupReady />)
  const backup = view.getByRole("textbox") as HTMLTextAreaElement
  expect(backup.readOnly).toBe(true)
  expect(backup.value).toBe(backupText)
  expect(view.getByRole("button", { name: "DONE" })).toBeDefined()
})

test("retired-key deletion requires real confirmation interaction", () => {
  const view = render(<identity.DeleteRetiredKey />)
  const submit = view.getByRole("button", { name: "DELETE KEY" }) as HTMLButtonElement
  expect(submit.disabled).toBe(true)
  fireEvent.click(view.getByRole("checkbox"))
  expect(submit.disabled).toBe(false)
})

test("NIP-46 form cannot submit until input and confirmation are supplied", () => {
  const view = render(<identity.SwitchToNip46 />)
  const submit = view.getByRole("button", { name: "SWITCH IDENTITY" }) as HTMLButtonElement
  expect(submit.disabled).toBe(true)
  fireEvent.change(view.getByRole("textbox", { name: "Bunker URI" }), { target: { value: "preview-only-not-a-signer" } })
  expect(submit.disabled).toBe(true)
  fireEvent.click(view.getByRole("checkbox"))
  expect(submit.disabled).toBe(false)
})
