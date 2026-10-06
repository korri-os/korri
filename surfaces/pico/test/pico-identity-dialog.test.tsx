import { afterEach, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { PicoSurface } from "../src/PicoSurface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import { backupText, identityFixture } from "../src/fixtures/named-states"
import {
  PICO_IDENTITY_BACKUP_ACTION,
  PICO_IDENTITY_SWITCH_NIP46_ACTION,
  picoIdentityRetiredDeleteAction,
} from "../src/pico-settings-view"
import type { PicoMessage } from "../src/state/messages"
import { initialNavigation, type PicoNavigation } from "../src/state/navigation"
import type { PicoRequest } from "../src/state/requests"
import { update } from "../src/state/update"

afterEach(cleanup)

const korri: SurfaceModel = { ...fixtureModel, identityManagement: identityFixture({ _tag: "Idle" }) }

function story(model: SurfaceModel, ...messages: PicoMessage[]) {
  let nav: PicoNavigation = initialNavigation({ _tag: "Settings" })
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = update(nav, message, model)
    nav = step.model
    requests.push(...step.requests)
  }
  const question = nav.settings?.question
  return { nav, requests, form: question?._tag === "Identity" ? question.form : undefined }
}

const open = (actionId: string) => ({ _tag: "PressedSettingAction", actionId }) as const
const dismissals = (requests: readonly PicoRequest[]) =>
  requests.filter(request => request._tag === "DismissIdentityStatus").length

// Korri answers every dismissal with a new model, which renders the surface
// again. Only a change of dialog may dismiss, or that loop never ends.
test("Korri's status is cleared when the dialog opens, changes or closes, and never by a republished model", () => {
  const opened = story(korri, open(PICO_IDENTITY_BACKUP_ACTION))
  expect(dismissals(opened.requests)).toBe(1)
  const typed = story(korri, open(PICO_IDENTITY_BACKUP_ACTION),
    { _tag: "EditedIdentity", field: "password", value: "correct horse" },
    { _tag: "KorriPublished" }, { _tag: "KorriPublished" })
  expect(dismissals(typed.requests)).toBe(1)
  expect(typed.form?.password).toBe("correct horse")
  expect(dismissals(story(korri, open(PICO_IDENTITY_BACKUP_ACTION), { _tag: "ClosedIdentity" }).requests)).toBe(2)
  expect(dismissals(story(korri, open(PICO_IDENTITY_BACKUP_ACTION), { _tag: "PressedSystem" }).requests)).toBe(2)
})

test("a new action starts an empty form", () => {
  const { form } = story(korri, open(PICO_IDENTITY_BACKUP_ACTION),
    { _tag: "EditedIdentity", field: "password", value: "secret" },
    { _tag: "ClosedIdentity" }, open(PICO_IDENTITY_BACKUP_ACTION))
  expect(form?.password).toBe("")
})

test("a submission sends what the form holds, and nothing while it is incomplete", () => {
  const sent = (...messages: PicoMessage[]) => story(korri, ...messages).requests
    .filter(request => request._tag !== "DismissIdentityStatus")
  expect(sent(open(PICO_IDENTITY_SWITCH_NIP46_ACTION), { _tag: "SubmittedIdentity" })).toEqual([])
  expect(sent(open(PICO_IDENTITY_SWITCH_NIP46_ACTION),
    { _tag: "EditedIdentity", field: "bunkerUri", value: "bunker://x" },
    { _tag: "ChoseIdentityDisposition", disposition: "delete" },
    { _tag: "CheckedIdentityConfirmation", confirmed: true },
    { _tag: "SubmittedIdentity" })).toEqual([
    { _tag: "SwitchIdentityToNip46", bunkerUri: "bunker://x", disposition: "delete", trustLossConfirmed: true },
  ])
  expect(sent(open(picoIdentityRetiredDeleteAction("npub-old")),
    { _tag: "CheckedIdentityConfirmation", confirmed: true }, { _tag: "SubmittedIdentity" })).toEqual([
    { _tag: "DeleteRetiredIdentity", publicKey: "npub-old", backupConfirmed: true },
  ])
})

test("nothing is sent while Korri is working", () => {
  const working: SurfaceModel = { ...korri, identityManagement: identityFixture({ _tag: "Working", operation: "Encrypting" }) }
  const { requests } = story(working, open(PICO_IDENTITY_BACKUP_ACTION),
    { _tag: "EditedIdentity", field: "password", value: "pw" }, { _tag: "SubmittedIdentity" })
  expect(requests.filter(request => request._tag === "ExportIdentityBackup")).toEqual([])
})

test("a backup Korri made is drawn once, and a stale drawing is ignored", () => {
  const ready: SurfaceModel = { ...korri, identityManagement: identityFixture({ _tag: "BackupReady", encryptedSecret: backupText }) }
  const { nav, requests } = story(ready, open(PICO_IDENTITY_BACKUP_ACTION), { _tag: "KorriPublished" })
  expect(requests.filter(request => request._tag === "RenderQr")).toEqual([{ _tag: "RenderQr", text: backupText }])
  const stale = update(nav, { _tag: "RenderedQr", text: "older", dataUrl: "data:old" }, ready).model
  const drawn = update(stale, { _tag: "RenderedQr", text: backupText, dataUrl: "data:qr" }, ready).model
  const question = drawn.settings?.question
  expect(question?._tag === "Identity" ? question.qr : undefined).toEqual({ text: backupText, dataUrl: "data:qr" })
})

test("through the surface, typing survives a republished model", () => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} initialView={{ _tag: "Settings" }} model={korri} />)
  fireEvent.click(screen.getByRole("tab", { name: "IDENTITY" }))
  fireEvent.click(screen.getByRole("button", { name: /Back up current identity/ }))
  const password = document.querySelector<HTMLInputElement>('.pico-identity-dialog input[type="password"]')
  if (password === null) throw new Error("Expected the backup password field")
  fireEvent.change(password, { target: { value: "correct horse" } })
  act(() => view.rerender(<PicoSurface host={host} initialView={{ _tag: "Settings" }} model={{ ...korri }} />))
  expect(document.querySelector<HTMLInputElement>('.pico-identity-dialog input[type="password"]')?.value).toBe("correct horse")
})
