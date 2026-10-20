/**
 * The SAVE DATA page, driven as a person drives it: by the words on screen,
 * through the real identity model and the one door to Korri, into a host
 * that records every argument it is handed. Each of C16 to C20 is asserted
 * by its words and by the host call it ends in.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { ComponentType } from "react"
import type { SurfaceHost, SurfaceIdentityManagement, SurfaceIdentityStatus } from "@contracts/surface/korri-surface"
import { createFixtureHost } from "../src/fixtures/fixture-host"
import { fixtureEncryptedSecret } from "../src/fixtures/fixture-save-data"
import { shortKey } from "../src/plan/save-data"
import { ZoneSaveData, type ZoneSaveDataEvent, type ZoneSaveDataFlow } from "../src/pages/ZoneSaveData"
import * as page from "../src/pages/ZoneSaveData.page.part"
import * as band from "../src/ui/organisms/ZoneSaveBand.organism.part"
import * as entry from "../src/ui/organisms/ZoneSaveEntry.organism.part"
import * as slab from "../src/ui/organisms/ZoneSecretSlab.organism.part"
import { IDENTITY_START, update, type IdentityFlow, type IdentityMessage, type IdentityModel } from "../src/state/identity"
import { useZoneProgram } from "../src/use-zone-program"
import { performZoneRequest } from "../src/zone-host"

afterEach(cleanup)

// The page takes the model's own values: the binding passes them unchanged.
const flowFits = (flow: IdentityFlow): ZoneSaveDataFlow => flow
const eventFits = (event: ZoneSaveDataEvent): IdentityMessage => event
void flowFits
void eventFits

const KEY_A = "npub1retiredaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaakeya"
const KEY_B = "npub1retiredbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbkeyb"
const OWNER = "npub1ownerccccccccccccccccccccccccccccccccccccccccccccccccccnewk"
/** A made-up NIP-49-shaped backup from the fixtures: generated, never a real key. */
const SECRET = fixtureEncryptedSecret

const identityOf = (status: SurfaceIdentityStatus = { _tag: "Idle" }, overrides: Partial<SurfaceIdentityManagement> = {}): SurfaceIdentityManagement =>
  ({ localBackupAvailable: false, retiredPublicKeys: [KEY_A, KEY_B], status, ...overrides })

/** The fixture host, recording every identity call with all its arguments. */
function recordingHost() {
  const calls: unknown[][] = []
  const base = createFixtureHost()
  const host: SurfaceHost = {
    ...base,
    exportIdentityBackup: (...args) => void calls.push(["exportIdentityBackup", ...args]),
    switchIdentityFromBackup: (...args) => void calls.push(["switchIdentityFromBackup", ...args]),
    switchIdentityToNip46: (...args) => void calls.push(["switchIdentityToNip46", ...args]),
    deleteRetiredIdentity: (...args) => void calls.push(["deleteRetiredIdentity", ...args]),
    dismissIdentityStatus: () => void calls.push(["dismissIdentityStatus"]),
  }
  return { host, calls }
}

let dispatchOutside: ((message: IdentityMessage) => void) | undefined

/** The binding a later stage writes: the program, the page, the one door. */
function Bound({ identity, host, start }: { identity: SurfaceIdentityManagement | undefined; host: SurfaceHost; start: IdentityModel }) {
  const program = useZoneProgram(() => start, update, identity, request => performZoneRequest(host, request))
  dispatchOutside = program.dispatch
  return (
    <ZoneSaveData
      card={program.model.card}
      flow={program.model.flow}
      identity={identity}
      onEvent={program.dispatch}
    />
  )
}

function mount(identity: SurfaceIdentityManagement | undefined, start: IdentityModel = IDENTITY_START) {
  const { host, calls } = recordingHost()
  const view = render(<Bound host={host} identity={identity} start={start} />)
  const publish = (next: SurfaceIdentityManagement | undefined) => {
    act(() => view.rerender(<Bound host={host} identity={next} start={start} />))
    act(() => dispatchOutside?.({ _tag: "KorriPublished" }))
  }
  const back = () => act(() => dispatchOutside?.({ _tag: "PressedBack" }))
  return { calls, publish, back }
}

const card = (name: RegExp) => screen.getByRole("button", { name })
const press = (name: string | RegExp) => fireEvent.click(screen.getByRole("button", { name }))
const typeKeys = (text: string) => {
  for (const char of text) press(char)
}
const dialog = () => screen.getByRole("dialog")

describe("the band", () => {
  test("BACK UP, two SWITCH cards, then one RETIRED card per key, shortened", () => {
    mount(identityOf())
    expect(screen.getByText("Save data")).toBeTruthy()
    expect(card(/Back up/)).toBeTruthy()
    expect(card(/From backup/)).toBeTruthy()
    expect(card(/Bunker/)).toBeTruthy()
    expect(screen.getAllByText("Retired").length).toBe(2)
    expect(screen.getByText(shortKey(KEY_A))).toBeTruthy()
    expect(screen.getByText(shortKey(KEY_B))).toBeTruthy()
    // No row until a retired key has the cursor.
    expect(screen.queryByRole("button", { name: "Delete" })).toBeNull()
  })

  test("BACK UP reads NEW without a local backup and BACKED UP with one", () => {
    mount(identityOf())
    expect(within(card(/Back up/)).getByText("New")).toBeTruthy()
    cleanup()
    mount(identityOf({ _tag: "Idle" }, { localBackupAvailable: true }))
    expect(within(card(/Back up/)).getByText("Backed up")).toBeTruthy()
  })

  test("the cursor goes to a step when it opens, and back to the card when it closes", () => {
    const { back } = mount(identityOf())
    press(/Bunker/)
    expect(document.activeElement?.textContent).toBe("1")
    back()
    expect(document.activeElement).toBe(card(/Bunker/))
  })

  test("focus on a card moves the cursor there", () => {
    mount(identityOf())
    fireEvent.focus(card(/Bunker/))
    expect(card(/Bunker/).dataset["focused"]).toBe("true")
    expect(card(/Back up/).dataset["focused"]).toBeUndefined()
  })

  test("without identity management: one NOT AVAILABLE card and nothing else", () => {
    const { calls } = mount(undefined)
    expect(screen.getByText("Not available")).toBeTruthy()
    expect(screen.getAllByRole("button").length).toBe(1)
    press(/Not available/)
    expect(screen.queryByRole("dialog")).toBeNull()
    expect(screen.queryByRole("group", { name: "Keyboard" })).toBeNull()
    expect(calls).toEqual([])
  })
})

describe("C16 back up", () => {
  test("the password is typed as dots and sent verbatim; the backup shows as a QR and grouped text; B closes it", () => {
    const { calls, publish, back } = mount(identityOf())
    press(/Back up/)
    expect(screen.getByText("Password")).toBeTruthy()
    typeKeys("hi")
    expect(screen.getByRole("group", { name: "Keyboard" }).textContent).toContain("••")
    expect(screen.queryByText("hi")).toBeNull()
    press("Done")
    expect(calls).toEqual([["exportIdentityBackup", "hi", undefined]])

    publish(identityOf({ _tag: "BackupReady", encryptedSecret: SECRET }, { localBackupAvailable: true }))
    // The QR leaves the card's well and takes the band's room, beside the slab.
    const qr = screen.getByRole("img", { name: "Backup QR code" })
    expect(qr.closest(".zone-save-data-backup")).not.toBeNull()
    expect(qr.closest(".zone-save-card")).toBeNull()
    expect(screen.queryByRole("button", { name: /Back up/ })).toBeNull()
    // The cursor rests on the slab.
    expect(document.activeElement).toBe(screen.getByRole("region", { name: "Backup secret" }))
    const text = screen.getByRole("region", { name: "Backup secret" })
    expect(text.textContent).toContain("ncry")
    expect(within(text).getByText("ptse")).toBeTruthy()

    back()
    expect(calls.at(-1)).toEqual(["dismissIdentityStatus"])
    publish(identityOf({ _tag: "Idle" }, { localBackupAvailable: true }))
    expect(within(card(/Back up/)).getByText("Backed up")).toBeTruthy()
    expect(screen.queryByRole("img", { name: "Backup QR code" })).toBeNull()
  })

  test("the QR code's paper keeps a quiet zone of four modules on every side", () => {
    render(<page.BackupReady />)
    const svg = screen.getByRole("img", { name: "Backup QR code" })
    const span = Number(svg.getAttribute("viewBox")?.split(" ")[2])
    const marks = [...(svg.querySelector("path")?.getAttribute("d") ?? "").matchAll(/M(\d+) (\d+)/g)]
      .map(match => [Number(match[1]), Number(match[2])] as const)
    expect(marks.length).toBeGreaterThan(0)
    expect(Math.min(...marks.map(([x]) => x))).toBe(4)
    expect(Math.min(...marks.map(([, y]) => y))).toBe(4)
    expect(Math.max(...marks.map(([x]) => x))).toBe(span - 5)
    expect(Math.max(...marks.map(([, y]) => y))).toBe(span - 5)
  })

  test("the 640x480 and 1280x300 states draw the backup in the band's room", () => {
    for (const State of [page.BackupReadyHandheld, page.BackupReadyStrip]) {
      render(<State />)
      expect(screen.getByRole("img", { name: "Backup QR code" }).closest(".zone-save-data-qr-square")).not.toBeNull()
      expect(screen.getByRole("region", { name: "Backup secret" })).toBeTruthy()
      cleanup()
    }
  })

  test("an empty password sends nothing", () => {
    const { calls } = mount(identityOf())
    press(/Back up/)
    press("Done")
    expect(calls).toEqual([])
  })
})

describe("C17 switch from backup", () => {
  test("secret, password, TRANSFER / DELETE, then the trust loss; YES sends all four arguments", () => {
    const { calls } = mount(identityOf())
    press(/From backup/)
    expect(screen.getByText("Backup")).toBeTruthy()
    typeKeys("ncryptsec1q")
    press("Done")
    expect(screen.getByText("Password")).toBeTruthy()
    typeKeys("pw")
    press("Done")

    expect(within(dialog()).getByText("Local records")).toBeTruthy()
    expect(within(dialog()).getByRole("button", { name: "Transfer" })).toBeTruthy()
    press("Delete")

    expect(within(dialog()).getByText("Switch key?")).toBeTruthy()
    expect(within(dialog()).getByText("Other devices will stop trusting this key")).toBeTruthy()
    expect(calls).toEqual([])
    press("Yes")
    expect(calls).toEqual([["switchIdentityFromBackup", "ncryptsec1q", "pw", "delete", true]])
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("NO on the trust loss sends nothing", () => {
    const { calls } = mount(identityOf())
    press(/From backup/)
    typeKeys("q")
    press("Done")
    typeKeys("p")
    press("Done")
    press("Transfer")
    press("No")
    expect(calls).toEqual([])
    expect(screen.queryByRole("dialog")).toBeNull()
  })

  test("focus moving across a dialog's buttons moves its cursor", () => {
    mount(identityOf(), { card: 1, flow: { _tag: "disposition", source: { kind: "bunker", bunkerUri: "bunker://x" }, focus: 0 } })
    const del = within(dialog()).getByRole("button", { name: "Delete" })
    fireEvent.focus(del)
    expect(del.dataset["focused"]).toBe("true")
  })
})

describe("C18 switch by bunker", () => {
  test("the URI layer with the prefix typed, then TRANSFER / DELETE and the trust loss; YES sends the URI", () => {
    const { calls } = mount(identityOf())
    press(/Bunker/)
    expect(screen.getByText("Bunker", { selector: ".zone-caps" })).toBeTruthy()
    const keyboard = screen.getByRole("group", { name: "Keyboard" })
    expect(keyboard.dataset["layer"]).toBe("uri")
    expect(keyboard.textContent).toContain("bunker://")
    typeKeys("ab")
    press("Done")
    press("Transfer")
    expect(within(dialog()).getByText("Other devices will stop trusting this key")).toBeTruthy()
    press("Yes")
    expect(calls).toEqual([["switchIdentityToNip46", "bunker://ab", "transfer", true]])
  })

  test("focus on a key moves the keyboard's cursor", () => {
    mount(identityOf(), { card: 2, flow: { _tag: "bunker", draft: { text: "bunker://", caret: 9, layer: "uri", focusedKey: "1" } } })
    const at = screen.getByRole("button", { name: "@" })
    fireEvent.focus(at)
    expect(at.dataset["focused"]).toBe("true")
    const done = screen.getByRole("button", { name: "Done" })
    fireEvent.focus(done)
    expect(done.dataset["focused"]).toBe("true")
  })
})

describe("C19 retired keys", () => {
  const retired = (index: number) => screen.getAllByRole("button", { name: /Retired/ })[index]!

  test("a focused retired key offers BACK UP and DELETE in a row under the band", () => {
    mount(identityOf())
    fireEvent.focus(retired(0))
    const row = document.querySelector(".zone-save-data-actions")!
    expect(within(row as HTMLElement).getByRole("button", { name: "Back up" })).toBeTruthy()
    expect(within(row as HTMLElement).getByRole("button", { name: "Delete" })).toBeTruthy()
    fireEvent.focus(card(/Bunker/))
    expect(within(row as HTMLElement).queryByRole("button")).toBeNull()
  })

  test("A on the card drops into the row on BACK UP, and focus there moves its cursor", () => {
    mount(identityOf())
    fireEvent.click(retired(1))
    const backUp = screen.getByRole("button", { name: "Back up" })
    expect(document.activeElement).toBe(backUp)
    expect(backUp.dataset["focused"]).toBe("on")
    const del = screen.getByRole("button", { name: "Delete" })
    fireEvent.focus(del)
    expect(del.dataset["focused"]).toBe("on")
    expect(backUp.dataset["focused"]).toBe("off")
  })

  test("BACK UP asks the password and sends it with the retired key", () => {
    const { calls } = mount(identityOf())
    fireEvent.click(retired(1))
    press("Back up")
    expect(screen.getByText("Password")).toBeTruthy()
    typeKeys("hi")
    press("Done")
    expect(calls).toEqual([["exportIdentityBackup", "hi", KEY_B]])
  })

  test("DELETE THIS KEY? then IS IT BACKED UP?; both YES delete that key with the backup confirmed", () => {
    const { calls } = mount(identityOf())
    fireEvent.click(retired(1))
    press("Delete")
    expect(within(dialog()).getByText("Delete this key?")).toBeTruthy()
    expect(within(dialog()).getByText("Back it up first.")).toBeTruthy()
    expect(within(dialog()).getByText(shortKey(KEY_B))).toBeTruthy()
    press("Yes")
    expect(calls).toEqual([])
    expect(within(dialog()).getByText("Is it backed up?")).toBeTruthy()
    press("Yes")
    expect(calls).toEqual([["deleteRetiredIdentity", KEY_B, true]])
  })

  test("NO on either question sends nothing", () => {
    const { calls } = mount(identityOf())
    fireEvent.click(retired(0))
    press("Delete")
    press("No")
    fireEvent.click(retired(0))
    press("Delete")
    press("Yes")
    press("No")
    expect(calls).toEqual([])
  })
})

describe("C20 identity status", () => {
  test("Working: Korri's operation as the kicker over the band, and nothing can be started", () => {
    const { calls } = mount(identityOf({ _tag: "Working", operation: "Encrypting backup" }))
    expect(within(screen.getByRole("status")).getByText("Encrypting backup")).toBeTruthy()
    press(/Back up/)
    expect(screen.queryByRole("group", { name: "Keyboard" })).toBeNull()
    expect(calls).toEqual([])
  })

  test("Problem: Korri's message verbatim with DISMISS", () => {
    const { calls } = mount(identityOf({ _tag: "Problem", message: "The password does not open this backup." }))
    expect(within(dialog()).getByText("The password does not open this backup.")).toBeTruthy()
    press("Dismiss")
    expect(calls).toEqual([["dismissIdentityStatus"]])
  })

  test("Switched: the new owner key shortened, with OK", () => {
    const { calls } = mount(identityOf({ _tag: "Switched", ownerPublicKey: OWNER }))
    expect(within(dialog()).getByText("Switched")).toBeTruthy()
    expect(within(dialog()).getByText(shortKey(OWNER))).toBeTruthy()
    press("OK")
    expect(calls).toEqual([["dismissIdentityStatus"]])
  })

  test("B dismisses Korri's dialog, and a republished model never does", () => {
    const { calls, publish, back } = mount(identityOf({ _tag: "Problem", message: "No" }))
    publish(identityOf({ _tag: "Problem", message: "No" }))
    expect(calls).toEqual([])
    back()
    expect(calls).toEqual([["dismissIdentityStatus"]])
  })
})

describe("the parts", () => {
  const statesOf = (part: Record<string, unknown>): [string, ComponentType][] =>
    Object.entries(part).filter(([key, value]) => /^[A-Z]/.test(key) && typeof value === "function") as [string, ComponentType][]

  test.each([
    ["ZoneSaveData", page],
    ["ZoneSaveBand", band],
    ["ZoneSaveEntry", entry],
    ["ZoneSecretSlab", slab],
  ] as const)("every state of %s renders words", (_, part) => {
    for (const [state, Component] of [["default", part.default as ComponentType] as const, ...statesOf(part)]) {
      render(<Component />)
      expect(`${state}: ${document.body.textContent?.trim().length ?? 0}`).not.toBe(`${state}: 0`)
      cleanup()
    }
  })

  test("the page part's states reach the cases they are named for", () => {
    const words = (Component: ComponentType) => {
      render(<Component />)
      const text = document.body.textContent ?? ""
      cleanup()
      return text
    }
    expect(words(page.NotAvailable)).toContain("Not available")
    expect(words(page.BackedUp)).toContain("Backed up")
    expect(words(page.Disposition)).toContain("Local records")
    expect(words(page.TrustLoss)).toContain("Other devices will stop trusting this key")
    expect(words(page.ConfirmDelete)).toContain("Back it up first.")
    expect(words(page.ConfirmBackedUp)).toContain("Is it backed up?")
    expect(words(page.Working)).toContain("Encrypting backup")
    expect(words(page.Switched)).toContain("Switched")
    expect(words(page.BackupReady)).toContain("ncry")
    expect(words(page.RetiredActions)).toContain("Delete")
    expect(words(page.RetiredPassword)).toContain("Password")
  })

  test("the page part answers presses through the real model", () => {
    const Default = page.default
    render(<Default />)
    fireEvent.click(screen.getAllByRole("button", { name: /Retired/ })[0]!)
    fireEvent.click(screen.getByRole("button", { name: "Delete" }))
    fireEvent.click(screen.getByRole("button", { name: "Yes" }))
    fireEvent.click(screen.getByRole("button", { name: "Yes" }))
    expect(screen.getAllByRole("button", { name: /Retired/ }).length).toBe(1)
  })
})
