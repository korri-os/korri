/**
 * Every OPTIONS rule, driven with plain values: one block per message, then
 * the `system` jump and the layer query the wire stage reads. The Wi-Fi
 * rules have their own file; here they are reached through the page's
 * messages.
 */
import { describe, expect, test } from "bun:test"
import {
  fixtureNetworkConnected,
  fixtureNetworkConnecting,
  fixtureNetworkNoAdapter,
  fixtureNetworkRejected,
  fixtureNetworkScanning,
  fixtureOptions,
  fixtureSeatsId,
} from "../src/fixtures/fixture-options"
import {
  optionsPlan,
  ZONE_OPTIONS_DISMISS,
  ZONE_OPTIONS_SYSTEM,
  ZONE_WIFI_HIDDEN,
  ZONE_WIFI_SCAN,
  zoneNetworkRowId,
  zoneSystemRowId,
  type ZoneOptionsKeyboard,
  type ZoneOptionsSlices,
} from "../src/plan/options"
import {
  focusSystemGroup,
  OPTIONS_START,
  optionsLayer,
  optionsStart,
  update,
  type ZoneOptions,
  type ZoneOptionsMessage,
} from "../src/state/options"

const korri = fixtureOptions
const at = (state: Partial<ZoneOptions>): ZoneOptions => ({ ...OPTIONS_START, ...state })
const after = (state: ZoneOptions, message: ZoneOptionsMessage, model: ZoneOptionsSlices = korri) => update(state, message, model)
const press = (rowId: string, state: ZoneOptions = OPTIONS_START, model: ZoneOptionsSlices = korri) =>
  after(state, { _tag: "PressedRow", rowId }, model)
const typing = (keyboard: Omit<ZoneOptionsKeyboard, "caret" | "layer"> & Partial<ZoneOptionsKeyboard>): ZoneOptions =>
  at({ keyboard: { caret: keyboard.draft.length, layer: "letters", ...keyboard } })
const nameEntry = { _tag: "Setting", settingId: "device-name", label: "Name", secret: false, maxLength: 64, placeholder: "This device" } as const
const keyEntry = {
  _tag: "Setting",
  settingId: "steamgriddb-credential",
  label: "SteamGridDB API key",
  secret: true,
  maxLength: 256,
  clearLabel: "Clear saved key",
  placeholder: "Paste API key",
} as const
const powerOff = zoneSystemRowId("power-off")

describe("the plan the state walks", () => {
  test("settings groups in Korri's order, then SYSTEM, then WI-FI", () => {
    expect(optionsPlan(korri).groups.map(group => group.title))
      .toEqual(["Device", "Controllers", "Metadata", "Games", "System information", "System", "WI-FI"])
  })
  test("SYSTEM is absent with no device actions, WI-FI absent with no network", () => {
    const titles = optionsPlan({ ...korri, actions: [], network: undefined }).groups.map(group => group.title)
    expect(titles).not.toContain("System")
    expect(titles).not.toContain("WI-FI")
  })
  test("a problem adds DISMISS under its row", () => {
    const rows = optionsPlan({ ...korri, settingsStatus: { _tag: "Problem", settingId: "device-name", message: "No." } }).rows
    expect(rows.map(row => row.id).slice(0, 2)).toEqual(["device-name", ZONE_OPTIONS_DISMISS])
  })
})

describe("FocusedRow", () => {
  test("puts the cursor on a row and remembers its place", () => {
    const next = after(OPTIONS_START, { _tag: "FocusedRow", rowId: fixtureSeatsId }).model
    expect(next.focused).toBe(fixtureSeatsId)
    expect(next.at).toBe(1)
  })
  test("ignores a row the page does not have", () => {
    expect(after(OPTIONS_START, { _tag: "FocusedRow", rowId: "nowhere" }).model).toBe(OPTIONS_START)
  })
  test("ignores the rows behind an open keyboard", () => {
    const state = typing({ entry: nameEntry, draft: "mini" })
    expect(after(state, { _tag: "FocusedRow", rowId: fixtureSeatsId }).model).toBe(state)
  })
})

describe("PressedRow", () => {
  test("a read-only row does nothing", () => {
    const step = press("korrid-version")
    expect(step.requests).toEqual([])
    expect(step.model.keyboard).toBeUndefined()
  })
  test("a choice steps forward, as right does", () => {
    expect(press(fixtureSeatsId).requests).toEqual([{ _tag: "ChangeSetting", settingId: fixtureSeatsId, value: "3" }])
  })
  test("a text setting raises the keyboard with Korri's value as the draft", () => {
    const keyboard = press("device-name").model.keyboard
    expect(keyboard?.entry).toEqual(nameEntry)
    expect(keyboard?.draft).toBe("mini")
    expect(keyboard?.caret).toBe(4)
  })
  test("a secret raises the keyboard empty and secret, with Korri's clear label", () => {
    const keyboard = press("steamgriddb-credential").model.keyboard
    expect(keyboard?.entry).toEqual(keyEntry)
    expect(keyboard?.draft).toBe("")
  })
  test("an action with nothing to confirm fires at once", () => {
    expect(press("game-folder-rescan").requests).toEqual([{ _tag: "RunAction", actionId: "game-folder-rescan" }])
  })
  test("an action Korri asks to confirm opens the dialog with its words, NO lit", () => {
    const step = press("game-folder:roms")
    expect(step.requests).toEqual([])
    expect(step.model.dialog).toEqual({
      actionId: "game-folder-remove:roms",
      confirm: {
        headline: "Remove game folder?",
        lines: ["Korri will remove games it added from this folder. Edited or hand-authored games stay."],
        confirmLabel: "Remove folder",
      },
      focused: 0,
    })
  })
  test("a destructive device action asks NO / YES under its label", () => {
    const step = press(powerOff)
    expect(step.requests).toEqual([])
    expect(step.model.dialog?.confirm).toEqual({ headline: "Power off", lines: [], confirmLabel: "Yes" })
  })
  test("a device action fires; an inert one does nothing", () => {
    expect(press(zoneSystemRowId("restart-korri")).requests).toEqual([{ _tag: "RunAction", actionId: "restart-korri" }])
    expect(press(zoneSystemRowId("sleep")).requests).toEqual([])
  })
  test("DISMISS asks Korri to dismiss its problem", () => {
    const problem: ZoneOptionsSlices = { ...korri, settingsStatus: { _tag: "Problem", settingId: "device-name", message: "No." } }
    expect(press(ZONE_OPTIONS_DISMISS, OPTIONS_START, problem).requests).toEqual([{ _tag: "DismissSettingsProblem" }])
  })
  test("a row Korri is saving does not act again", () => {
    const saving: ZoneOptionsSlices = { ...korri, settingsStatus: { _tag: "Saving", settingId: fixtureSeatsId } }
    expect(press(fixtureSeatsId, OPTIONS_START, saving).requests).toEqual([])
  })
  test("SCAN asks for a scan unless one is running", () => {
    expect(press(ZONE_WIFI_SCAN).requests).toEqual([{ _tag: "ScanWifi" }])
    expect(press(ZONE_WIFI_SCAN, OPTIONS_START, { ...korri, network: fixtureNetworkScanning }).requests).toEqual([])
  })
  test("a known network joins at once; a secured unknown one asks its password", () => {
    expect(press(zoneNetworkRowId("ap-cafe")).requests).toEqual([{ _tag: "ConnectWifi", networkId: "ap-cafe" }])
    const asked = press(zoneNetworkRowId("ap-attic"))
    expect(asked.requests).toEqual([])
    expect(asked.model.keyboard?.entry).toEqual({ _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" })
  })
  test("HIDDEN NETWORK asks the name first", () => {
    expect(press(ZONE_WIFI_HIDDEN).model.keyboard?.entry).toEqual({ _tag: "HiddenSsid" })
  })
  test("does nothing while a layer is open", () => {
    const state = at({ dialog: { actionId: "power-off", confirm: { headline: "Power off", lines: [], confirmLabel: "Yes" }, focused: 0 } })
    expect(press("game-folder-rescan", state).requests).toEqual([])
  })
})

describe("SteppedRow", () => {
  test("left and right step the choices round the ends", () => {
    const left = after(OPTIONS_START, { _tag: "SteppedRow", rowId: fixtureSeatsId, direction: -1 })
    expect(left.requests).toEqual([{ _tag: "ChangeSetting", settingId: fixtureSeatsId, value: "1" }])
    const four: ZoneOptionsSlices = {
      ...korri,
      settings: korri.settings.map(group => ({
        ...group,
        items: group.items.map(item => (item.id === fixtureSeatsId ? { ...item, value: "4" } : item)),
      })),
    }
    const right = after(OPTIONS_START, { _tag: "SteppedRow", rowId: fixtureSeatsId, direction: 1 }, four)
    expect(right.requests).toEqual([{ _tag: "ChangeSetting", settingId: fixtureSeatsId, value: "1" }])
  })
  test("a row that is not a choice does not step", () => {
    expect(after(OPTIONS_START, { _tag: "SteppedRow", rowId: "device-name", direction: 1 }).requests).toEqual([])
  })
})

describe("FocusedKey", () => {
  test("follows the cursor across the keys", () => {
    const next = after(typing({ entry: nameEntry, draft: "" }), { _tag: "FocusedKey", keyId: "q" }).model
    expect(next.keyboard?.focusedKey).toBe("q")
  })
  test("without a keyboard does nothing", () => {
    expect(after(OPTIONS_START, { _tag: "FocusedKey", keyId: "q" }).model).toBe(OPTIONS_START)
  })
})

describe("PressedKey", () => {
  test("a character goes in at the caret", () => {
    const next = after(typing({ entry: nameEntry, draft: "mni", caret: 1 }), { _tag: "PressedKey", key: { kind: "char", char: "i" } })
    expect(next.model.keyboard?.draft).toBe("mini")
    expect(next.model.keyboard?.caret).toBe(2)
  })
  test("space types a space; delete takes the character before the caret", () => {
    const spaced = after(typing({ entry: nameEntry, draft: "a" }), { _tag: "PressedKey", key: { kind: "space" } }).model
    expect(spaced.keyboard?.draft).toBe("a ")
    const deleted = after(typing({ entry: nameEntry, draft: "ab" }), { _tag: "PressedKey", key: { kind: "delete" } }).model
    expect(deleted.keyboard?.draft).toBe("a")
  })
  test("delete at the start and a character past the limit change nothing", () => {
    const start = typing({ entry: nameEntry, draft: "ab", caret: 0 })
    expect(after(start, { _tag: "PressedKey", key: { kind: "delete" } }).model).toBe(start)
    const full = typing({ entry: { ...nameEntry, maxLength: 2 }, draft: "ab" })
    expect(after(full, { _tag: "PressedKey", key: { kind: "char", char: "c" } }).model).toBe(full)
  })
  test("a layer key switches the layer", () => {
    const next = after(typing({ entry: nameEntry, draft: "" }), { _tag: "PressedKey", key: { kind: "layer", layer: "symbols" } })
    expect(next.model.keyboard?.layer).toBe("symbols")
  })
  test("DONE commits a text setting and closes", () => {
    const next = after(typing({ entry: nameEntry, draft: "zao" }), { _tag: "PressedKey", key: { kind: "done" } })
    expect(next.requests).toEqual([{ _tag: "ChangeSetting", settingId: "device-name", value: "zao" }])
    expect(next.model.keyboard).toBeUndefined()
  })
  test("DONE on an empty secret closes without a word to Korri", () => {
    const next = after(typing({ entry: keyEntry, draft: "" }), { _tag: "PressedKey", key: { kind: "done" } })
    expect(next.requests).toEqual([])
    expect(next.model.keyboard).toBeUndefined()
  })
  test("DONE on a network's password joins it", () => {
    const state = typing({ entry: { _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" }, draft: "tide pool" })
    expect(after(state, { _tag: "PressedKey", key: { kind: "done" } }).requests)
      .toEqual([{ _tag: "ConnectWifi", networkId: "ap-attic", password: "tide pool" }])
  })
  test("DONE on a hidden name asks its password; on that password, joins", () => {
    const name = after(typing({ entry: { _tag: "HiddenSsid" }, draft: "attic-ap" }), { _tag: "PressedKey", key: { kind: "done" } })
    expect(name.model.keyboard?.entry).toEqual({ _tag: "HiddenPassword", ssid: "attic-ap" })
    expect(name.model.keyboard?.draft).toBe("")
    const open = after(name.model, { _tag: "PressedKey", key: { kind: "done" } })
    expect(open.requests).toEqual([{ _tag: "ConnectHiddenWifi", ssid: "attic-ap" }])
  })
})

describe("PressedClear", () => {
  test("sends an empty secret and closes", () => {
    const next = after(typing({ entry: keyEntry, draft: "half" }), { _tag: "PressedClear" })
    expect(next.requests).toEqual([{ _tag: "ChangeSetting", settingId: "steamgriddb-credential", value: "" }])
    expect(next.model.keyboard).toBeUndefined()
  })
  test("does nothing for a text setting", () => {
    expect(after(typing({ entry: nameEntry, draft: "x" }), { _tag: "PressedClear" }).requests).toEqual([])
  })
})

describe("the dialog", () => {
  const dialog = press("game-folder:roms").model
  test("FocusedDialogButton follows the cursor", () => {
    expect(after(dialog, { _tag: "FocusedDialogButton", index: 1 }).model.dialog?.focused).toBe(1)
  })
  test("NO closes without acting", () => {
    const next = after(dialog, { _tag: "PressedDialogButton", index: 0 })
    expect(next.requests).toEqual([])
    expect(next.model.dialog).toBeUndefined()
  })
  test("YES fires the action and closes", () => {
    const next = after(dialog, { _tag: "PressedDialogButton", index: 1 })
    expect(next.requests).toEqual([{ _tag: "RunAction", actionId: "game-folder-remove:roms" }])
    expect(next.model.dialog).toBeUndefined()
  })
})

describe("PressedBack", () => {
  test("closes the keyboard without sending the draft", () => {
    const next = after(typing({ entry: nameEntry, draft: "zao" }), { _tag: "PressedBack" })
    expect(next.requests).toEqual([])
    expect(next.model.keyboard).toBeUndefined()
  })
  test("closes the dialog without acting", () => {
    const next = after(press(powerOff).model, { _tag: "PressedBack" })
    expect(next.requests).toEqual([])
    expect(next.model.dialog).toBeUndefined()
  })
  test("with nothing open does nothing", () => {
    expect(after(OPTIONS_START, { _tag: "PressedBack" }).model).toBe(OPTIONS_START)
  })
})

describe("PressedMenu", () => {
  test("commits the draft, as DONE does", () => {
    expect(after(typing({ entry: nameEntry, draft: "zao" }), { _tag: "PressedMenu" }).requests)
      .toEqual([{ _tag: "ChangeSetting", settingId: "device-name", value: "zao" }])
  })
  test("with no keyboard does nothing", () => {
    expect(after(OPTIONS_START, { _tag: "PressedMenu" }).model).toBe(OPTIONS_START)
  })
})

describe("KorriPublished", () => {
  test("a fresh rejected key reopens the keyboard for that network, once", () => {
    const connecting: ZoneOptionsSlices = { ...korri, network: fixtureNetworkConnecting }
    const rejected: ZoneOptionsSlices = { ...korri, network: fixtureNetworkRejected }
    const reopened = after(optionsStart(connecting), { _tag: "KorriPublished" }, rejected).model
    expect(reopened.keyboard?.entry).toEqual({ _tag: "WifiPassword", networkId: "ap-attic", ssid: "Attic 5G" })
    expect(reopened.focused).toBe(zoneNetworkRowId("ap-attic"))
    const cancelled = after(reopened, { _tag: "PressedBack" }, rejected).model
    expect(after(cancelled, { _tag: "KorriPublished" }, rejected).model.keyboard).toBeUndefined()
  })
  test("a failure from before the page opened raises nothing", () => {
    const rejected: ZoneOptionsSlices = { ...korri, network: fixtureNetworkRejected }
    expect(after(optionsStart(rejected), { _tag: "KorriPublished" }, rejected).model.keyboard).toBeUndefined()
  })
  test("the cursor on a row Korri removed moves to the row now in its place", () => {
    const problem: ZoneOptionsSlices = { ...korri, settingsStatus: { _tag: "Problem", settingId: "device-name", message: "No." } }
    const onDismiss = after(OPTIONS_START, { _tag: "FocusedRow", rowId: ZONE_OPTIONS_DISMISS }, problem).model
    expect(after(onDismiss, { _tag: "KorriPublished" }, korri).model.focused).toBe(fixtureSeatsId)
  })
  test("a setting's keyboard closes when Korri stops offering the text", () => {
    const readOnly: ZoneOptionsSlices = {
      ...korri,
      settings: korri.settings.map(group => ({
        ...group,
        items: group.items.map(item => (item.id === "device-name" ? { id: item.id, label: item.label } : item)),
      })),
    }
    expect(after(press("device-name").model, { _tag: "KorriPublished" }, readOnly).model.keyboard).toBeUndefined()
  })
  test("a dialog closes when its action is gone", () => {
    expect(after(press(powerOff).model, { _tag: "KorriPublished" }, { ...korri, actions: [] }).model.dialog).toBeUndefined()
  })
  test("a publish that changes nothing keeps the same value", () => {
    const state = optionsStart(korri)
    expect(after(state, { _tag: "KorriPublished" }, { ...korri, network: fixtureNetworkConnected }).model).toBe(state)
  })
})

describe("focusSystemGroup", () => {
  test("lands on SYSTEM's first row", () => {
    const next = focusSystemGroup(OPTIONS_START)
    expect(next.focused).toBe(ZONE_OPTIONS_SYSTEM)
    expect(after(next, { _tag: "KorriPublished" }).model.focused).toBe(zoneSystemRowId("controllers-permission"))
  })
  test("lands on the first row when there is no SYSTEM group", () => {
    const next = after(focusSystemGroup(OPTIONS_START), { _tag: "KorriPublished" }, { ...korri, actions: [] }).model
    expect(next.focused).toBe("device-name")
  })
  test("leaves an open keyboard alone", () => {
    const state = typing({ entry: nameEntry, draft: "z" })
    expect(focusSystemGroup(state)).toBe(state)
  })
})

describe("optionsLayer", () => {
  test("names what is over the page", () => {
    expect(optionsLayer(OPTIONS_START)).toBe("none")
    expect(optionsLayer(press("device-name").model)).toBe("keyboard")
    expect(optionsLayer(press(powerOff).model)).toBe("dialog")
  })
})

describe("no adapter", () => {
  test("WI-FI is the connection and NO WI-FI ADAPTER, nothing to press", () => {
    const plan = optionsPlan({ ...korri, network: fixtureNetworkNoAdapter })
    const wifi = plan.groups.find(group => group.title === "WI-FI")
    expect(wifi?.rows.map(row => row.item.label)).toEqual(["CONNECTION", "NO WI-FI ADAPTER"])
    expect(wifi?.rows.every(row => row.press._tag === "Nothing")).toBe(true)
  })
})
