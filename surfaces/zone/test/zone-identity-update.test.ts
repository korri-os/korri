/**
 * Every identity rule, driven with plain values. One block per message,
 * then the flows end to end.
 */
import { describe, expect, test } from "bun:test"
import type { SurfaceIdentityManagement, SurfaceIdentityStatus } from "@contracts/surface/korri-surface"
import {
  IDENTITY_BUNKER_PREFIX,
  IDENTITY_START,
  identityCardCount,
  retiredKeyAt,
  update,
  type IdentityDraft,
  type IdentityFlow,
  type IdentityMessage,
  type IdentityModel,
  type IdentitySource,
} from "../src/state/identity"
import type { ZoneRequest } from "../src/state/requests"
import { fixtureEncryptedSecret } from "../src/fixtures/fixture-save-data"

const KEY_A = "npub1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
const KEY_B = "npub1bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
/** A made-up NIP-49-shaped backup from the fixtures: generated, never a real key. */
const SECRET = fixtureEncryptedSecret

const identityOf = (status: SurfaceIdentityStatus = { _tag: "Idle" }, retired: readonly string[] = [KEY_A, KEY_B]): SurfaceIdentityManagement =>
  ({ localBackupAvailable: false, retiredPublicKeys: retired, status })

const idle = identityOf()
const working = identityOf({ _tag: "Working", operation: "Encrypting" })
const ready = identityOf({ _tag: "BackupReady", encryptedSecret: SECRET })
const problem = identityOf({ _tag: "Problem", message: "Wrong password" })
const switched = identityOf({ _tag: "Switched", ownerPublicKey: KEY_B })

const draft = (text: string, layer: IdentityDraft["layer"] = "letters", focusedKey = "q"): IdentityDraft =>
  ({ text, caret: text.length, layer, focusedKey })
const at = (flow: IdentityFlow, card = 0): IdentityModel => ({ card, flow })
const after = (model: IdentityModel, message: IdentityMessage, identity: SurfaceIdentityManagement | undefined = idle) =>
  update(model, message, identity)
/** Without identity management: a default parameter would swallow an explicit undefined. */
const withoutIdentity = (model: IdentityModel, message: IdentityMessage) => update(model, message, undefined)
const run = (identity: SurfaceIdentityManagement | undefined, ...messages: IdentityMessage[]) => {
  let model = IDENTITY_START
  const requests: ZoneRequest[] = []
  for (const message of messages) {
    const step = update(model, message, identity)
    model = step.model
    requests.push(...step.requests)
  }
  return { model, requests }
}
const type = (text: string): IdentityMessage[] => [...text].map(char => ({ _tag: "TypedChar", char }) as const)

const backupSource: IdentitySource = { kind: "backup", encryptedSecret: SECRET, password: "pw" }
const bunkerSource: IdentitySource = { kind: "bunker", bunkerUri: "bunker://abc?relay=wss://r" }

describe("cards", () => {
  test("three fixed cards then one per retired key; one card without identity management", () => {
    expect(identityCardCount(idle)).toBe(5)
    expect(identityCardCount(identityOf({ _tag: "Idle" }, []))).toBe(3)
    expect(identityCardCount(undefined)).toBe(1)
    expect(retiredKeyAt(idle, 3)).toBe(KEY_A)
    expect(retiredKeyAt(idle, 4)).toBe(KEY_B)
    expect(retiredKeyAt(idle, 2)).toBeUndefined()
    expect(retiredKeyAt(undefined, 3)).toBeUndefined()
  })
})

describe("FocusedSaveCard", () => {
  test("moves the cursor", () => {
    expect(after(IDENTITY_START, { _tag: "FocusedSaveCard", index: 2 }).model.card).toBe(2)
  })
  test("the same card changes nothing", () => {
    expect(after(IDENTITY_START, { _tag: "FocusedSaveCard", index: 0 }).model).toBe(IDENTITY_START)
  })
  test("is ignored under a keyboard or a dialog", () => {
    const model = at({ _tag: "confirmDelete", publicKey: KEY_A, focus: 0 }, 3)
    expect(after(model, { _tag: "FocusedSaveCard", index: 1 }).model).toBe(model)
  })
  test("is clamped to the last card", () => {
    expect(after(IDENTITY_START, { _tag: "FocusedSaveCard", index: 40 }).model.card).toBe(4)
  })
})

describe("FocusedSecret", () => {
  test("the cursor on the backup's slab", () => {
    expect(after(at({ _tag: "showSecret", onSlab: false }), { _tag: "FocusedSecret" }, ready).model.flow)
      .toEqual({ _tag: "showSecret", onSlab: true })
  })
  test("Korri's backup with no flow behind it is the same fact", () => {
    expect(after(IDENTITY_START, { _tag: "FocusedSecret" }, ready).model.flow).toEqual({ _tag: "showSecret", onSlab: true })
    expect(after(IDENTITY_START, { _tag: "FocusedSecret" }, idle).model).toBe(IDENTITY_START)
  })
  test("already there changes nothing", () => {
    const model = at({ _tag: "showSecret", onSlab: true })
    expect(after(model, { _tag: "FocusedSecret" }, ready).model).toBe(model)
  })
  test("a card focused from the slab takes the cursor back to the band", () => {
    expect(after(at({ _tag: "showSecret", onSlab: true }), { _tag: "FocusedSaveCard", index: 0 }, ready).model)
      .toEqual(at({ _tag: "showSecret", onSlab: false }))
  })
})

describe("ChoseSaveCard", () => {
  test("BACK UP asks a password on the secret keyboard", () => {
    const { model } = after(IDENTITY_START, { _tag: "ChoseSaveCard", index: 0 })
    expect(model.flow).toEqual({ _tag: "password", for: { kind: "export" }, draft: draft("") })
  })
  test("SWITCH (FROM BACKUP) asks the encrypted secret", () => {
    expect(after(IDENTITY_START, { _tag: "ChoseSaveCard", index: 1 }).model.flow._tag).toBe("secret")
  })
  test("SWITCH (BUNKER) opens the URI layer with the bunker prefix typed", () => {
    const { model } = after(IDENTITY_START, { _tag: "ChoseSaveCard", index: 2 })
    expect(model.flow).toEqual({ _tag: "bunker", draft: draft(IDENTITY_BUNKER_PREFIX, "uri", "1") })
  })
  test("a retired card asks the first delete question about its own key", () => {
    const { model, requests } = after(IDENTITY_START, { _tag: "ChoseSaveCard", index: 4 })
    expect(model).toEqual({ card: 4, flow: { _tag: "confirmDelete", publicKey: KEY_B, focus: 0 } })
    expect(requests).toEqual([])
  })
  test("nothing starts while Korri works, or under its problem or switched dialog", () => {
    for (const identity of [working, problem, switched]) {
      expect(after(IDENTITY_START, { _tag: "ChoseSaveCard", index: 0 }, identity).model).toBe(IDENTITY_START)
    }
  })
  test("nothing starts without identity management", () => {
    expect(withoutIdentity(IDENTITY_START, { _tag: "ChoseSaveCard", index: 0 }).model).toBe(IDENTITY_START)
  })
  test("nothing starts over a flow already open", () => {
    const model = at({ _tag: "secret", draft: draft("ab") }, 1)
    expect(after(model, { _tag: "ChoseSaveCard", index: 0 }).model).toBe(model)
  })
  test("a finished backup on screen is dismissed when another flow starts", () => {
    const { model, requests } = after(at({ _tag: "showSecret", onSlab: false }), { _tag: "ChoseSaveCard", index: 2 }, ready)
    expect(model.flow._tag).toBe("bunker")
    expect(requests).toEqual([{ _tag: "DismissIdentityStatus" }])
  })
})

describe("keyboard messages", () => {
  const password = at({ _tag: "password", for: { kind: "export" }, draft: draft("ab") })
  test("FocusedKey moves the key cursor", () => {
    const { model } = after(password, { _tag: "FocusedKey", key: "z" })
    expect(model.flow._tag === "password" && model.flow.draft.focusedKey).toBe("z")
  })
  test("FocusedKey on the same key changes nothing", () => {
    expect(after(password, { _tag: "FocusedKey", key: "q" }).model).toBe(password)
  })
  test("TypedChar inserts at the caret", () => {
    const { model } = after(password, { _tag: "TypedChar", char: "c" })
    expect(model.flow._tag === "password" && model.flow.draft).toEqual({ text: "abc", caret: 3, layer: "letters", focusedKey: "c" })
  })
  test("TypedSpace inserts a space", () => {
    const { model } = after(password, { _tag: "TypedSpace" })
    expect(model.flow._tag === "password" && model.flow.draft.text).toBe("ab ")
  })
  test("DeletedChar removes the character before the caret, and nothing at the start", () => {
    const { model } = after(password, { _tag: "DeletedChar" })
    expect(model.flow._tag === "password" && model.flow.draft.text).toBe("a")
    const empty = at({ _tag: "secret", draft: draft("") })
    const next = after(empty, { _tag: "DeletedChar" }).model
    expect(next.flow._tag === "secret" && next.flow.draft).toEqual({ text: "", caret: 0, layer: "letters", focusedKey: "delete" })
  })
  test("SwitchedLayer changes the layer", () => {
    const { model } = after(password, { _tag: "SwitchedLayer", layer: "symbols" })
    expect(model.flow._tag === "password" && model.flow.draft.layer).toBe("symbols")
  })
  test("are ignored with no keyboard open", () => {
    expect(after(IDENTITY_START, { _tag: "TypedChar", char: "x" }).model).toBe(IDENTITY_START)
    expect(after(IDENTITY_START, { _tag: "FocusedKey", key: "x" }).model).toBe(IDENTITY_START)
  })
})

describe("PressedDone", () => {
  test("an empty password goes nowhere", () => {
    const model = at({ _tag: "password", for: { kind: "export" }, draft: draft("   ") })
    expect(after(model, { _tag: "PressedDone" })).toEqual({ model, requests: [] })
  })
  test("a backup password asks Korri for the backup, verbatim, and waits for it", () => {
    const { model, requests } = after(at({ _tag: "password", for: { kind: "export" }, draft: draft(" pass word ") }), { _tag: "PressedDone" })
    expect(model.flow).toEqual({ _tag: "showSecret", onSlab: false })
    expect(requests).toEqual([{ _tag: "ExportIdentityBackup", password: " pass word " }])
  })
  test("nothing is sent while Korri works", () => {
    const model = at({ _tag: "password", for: { kind: "export" }, draft: draft("pw") })
    expect(after(model, { _tag: "PressedDone" }, working)).toEqual({ model, requests: [] })
  })
  test("a secret, without its whitespace, moves on to its password", () => {
    const { model } = after(at({ _tag: "secret", draft: draft(` ${SECRET.slice(0, 20)} ${SECRET.slice(20)} `) }, 1), { _tag: "PressedDone" })
    expect(model.flow).toEqual({ _tag: "password", for: { kind: "switch", encryptedSecret: SECRET }, draft: draft("") })
  })
  test("an empty secret goes nowhere", () => {
    const model = at({ _tag: "secret", draft: draft("  ") })
    expect(after(model, { _tag: "PressedDone" }).model).toBe(model)
  })
  test("the backup's password moves on to the disposition, TRANSFER under the cursor", () => {
    const { model, requests } = after(at({ _tag: "password", for: { kind: "switch", encryptedSecret: SECRET }, draft: draft("pw") }), { _tag: "PressedDone" })
    expect(model.flow).toEqual({ _tag: "disposition", source: backupSource, focus: 0 })
    expect(requests).toEqual([])
  })
  test("a bunker URI moves on to the disposition; the bare prefix goes nowhere", () => {
    const bare = at({ _tag: "bunker", draft: draft(IDENTITY_BUNKER_PREFIX, "uri") })
    expect(after(bare, { _tag: "PressedDone" }).model).toBe(bare)
    const { model } = after(at({ _tag: "bunker", draft: draft(` ${bunkerSource.bunkerUri} `, "uri") }), { _tag: "PressedDone" })
    expect(model.flow).toEqual({ _tag: "disposition", source: bunkerSource, focus: 0 })
  })
  test("is ignored with no keyboard open", () => {
    expect(after(IDENTITY_START, { _tag: "PressedDone" }).model).toBe(IDENTITY_START)
  })
})

describe("FocusedChoice", () => {
  test("moves a dialog's cursor", () => {
    for (const flow of [
      { _tag: "disposition", source: backupSource, focus: 0 },
      { _tag: "trust", source: backupSource, disposition: "delete", focus: 0 },
      { _tag: "confirmDelete", publicKey: KEY_A, focus: 0 },
      { _tag: "confirmBackedUp", publicKey: KEY_A, focus: 0 },
    ] as const) {
      const { model } = after(at(flow), { _tag: "FocusedChoice", index: 1 })
      expect("focus" in model.flow && model.flow.focus).toBe(1)
    }
  })
  test("the same button changes nothing; no dialog changes nothing", () => {
    const model = at({ _tag: "confirmDelete", publicKey: KEY_A, focus: 0 })
    expect(after(model, { _tag: "FocusedChoice", index: 0 }).model).toBe(model)
    expect(after(IDENTITY_START, { _tag: "FocusedChoice", index: 1 }).model).toBe(IDENTITY_START)
  })
})

describe("ChoseDisposition", () => {
  test("carries the choice to the trust dialog, NO under the cursor", () => {
    const { model, requests } = after(at({ _tag: "disposition", source: bunkerSource, focus: 1 }), { _tag: "ChoseDisposition", disposition: "delete" })
    expect(model.flow).toEqual({ _tag: "trust", source: bunkerSource, disposition: "delete", focus: 0 })
    expect(requests).toEqual([])
  })
  test("is ignored outside the disposition dialog", () => {
    expect(after(IDENTITY_START, { _tag: "ChoseDisposition", disposition: "delete" }).model).toBe(IDENTITY_START)
  })
})

describe("Answered", () => {
  test("YES on the trust loss switches from the backup with trust loss confirmed", () => {
    const { model, requests } = after(at({ _tag: "trust", source: backupSource, disposition: "transfer", focus: 1 }, 1), { _tag: "Answered", yes: true })
    expect(model).toEqual({ card: 1, flow: { _tag: "idle" } })
    expect(requests).toEqual([{
      _tag: "SwitchIdentityFromBackup", encryptedSecret: SECRET, password: "pw", disposition: "transfer", trustLossConfirmed: true,
    }])
  })
  test("YES on the trust loss switches to the bunker with trust loss confirmed", () => {
    const { requests } = after(at({ _tag: "trust", source: bunkerSource, disposition: "delete", focus: 1 }), { _tag: "Answered", yes: true })
    expect(requests).toEqual([{ _tag: "SwitchIdentityToNip46", bunkerUri: bunkerSource.bunkerUri, disposition: "delete", trustLossConfirmed: true }])
  })
  test("NO on the trust loss abandons the switch and sends nothing", () => {
    expect(after(at({ _tag: "trust", source: bunkerSource, disposition: "delete", focus: 0 }), { _tag: "Answered", yes: false }))
      .toEqual({ model: IDENTITY_START, requests: [] })
  })
  test("nothing is switched while Korri works", () => {
    const model = at({ _tag: "trust", source: bunkerSource, disposition: "delete", focus: 1 })
    expect(after(model, { _tag: "Answered", yes: true }, working)).toEqual({ model, requests: [] })
  })
  test("YES on DELETE THIS KEY? only asks the second question", () => {
    const { model, requests } = after(at({ _tag: "confirmDelete", publicKey: KEY_A, focus: 1 }, 3), { _tag: "Answered", yes: true })
    expect(model.flow).toEqual({ _tag: "confirmBackedUp", publicKey: KEY_A, focus: 0 })
    expect(requests).toEqual([])
  })
  test("YES on IS IT BACKED UP? deletes that key with the backup confirmed", () => {
    const { model, requests } = after(at({ _tag: "confirmBackedUp", publicKey: KEY_A, focus: 1 }, 3), { _tag: "Answered", yes: true })
    expect(model.flow).toEqual({ _tag: "idle" })
    expect(requests).toEqual([{ _tag: "DeleteRetiredIdentity", publicKey: KEY_A, backupConfirmed: true }])
  })
  test("NO on either delete question sends nothing", () => {
    expect(after(at({ _tag: "confirmDelete", publicKey: KEY_A, focus: 0 }, 3), { _tag: "Answered", yes: false }))
      .toEqual({ model: at({ _tag: "idle" }, 3), requests: [] })
    expect(after(at({ _tag: "confirmBackedUp", publicKey: KEY_A, focus: 0 }, 3), { _tag: "Answered", yes: false }))
      .toEqual({ model: at({ _tag: "idle" }, 3), requests: [] })
  })
  test("nothing is deleted while Korri works", () => {
    const model = at({ _tag: "confirmBackedUp", publicKey: KEY_A, focus: 1 }, 3)
    expect(after(model, { _tag: "Answered", yes: true }, working)).toEqual({ model, requests: [] })
  })
  test("is ignored with no question open", () => {
    expect(after(IDENTITY_START, { _tag: "Answered", yes: true })).toEqual({ model: IDENTITY_START, requests: [] })
  })
})

describe("DismissedStatus", () => {
  test("clears Korri's problem, switched or backup status, and ends a flow waiting on it", () => {
    for (const identity of [problem, switched, ready]) {
      expect(after(at({ _tag: "showSecret", onSlab: false }), { _tag: "DismissedStatus" }, identity))
        .toEqual({ model: IDENTITY_START, requests: [{ _tag: "DismissIdentityStatus" }] })
    }
  })
  test("keeps a draft in progress under a problem", () => {
    const model = at({ _tag: "secret", draft: draft("ncrypt") }, 1)
    expect(after(model, { _tag: "DismissedStatus" }, problem)).toEqual({ model, requests: [{ _tag: "DismissIdentityStatus" }] })
  })
  test("asks nothing while Korri is idle or working", () => {
    for (const identity of [idle, working]) {
      expect(after(IDENTITY_START, { _tag: "DismissedStatus" }, identity).requests).toEqual([])
    }
  })
})

describe("PressedBack", () => {
  test("closes Korri's dialogs and a finished backup, as DISMISS does", () => {
    for (const identity of [problem, switched, ready]) {
      expect(after(at({ _tag: "showSecret", onSlab: false }), { _tag: "PressedBack" }, identity).requests).toEqual([{ _tag: "DismissIdentityStatus" }])
    }
  })
  test("on the band changes nothing", () => {
    expect(after(IDENTITY_START, { _tag: "PressedBack" })).toEqual({ model: IDENTITY_START, requests: [] })
  })
  test("leaves the first step of every flow", () => {
    for (const flow of [
      { _tag: "password", for: { kind: "export" }, draft: draft("pw") },
      { _tag: "secret", draft: draft("ab") },
      { _tag: "bunker", draft: draft("bunker://", "uri") },
      { _tag: "confirmDelete", publicKey: KEY_A, focus: 0 },
      { _tag: "showSecret", onSlab: false },
    ] as const) {
      expect(after(at(flow, 2), { _tag: "PressedBack" }).model).toEqual(at({ _tag: "idle" }, 2))
    }
  })
  test("steps back one step with what was typed kept", () => {
    expect(after(at({ _tag: "password", for: { kind: "switch", encryptedSecret: SECRET }, draft: draft("pw") }), { _tag: "PressedBack" }).model.flow)
      .toEqual({ _tag: "secret", draft: draft(SECRET, "letters", "done") })
    expect(after(at({ _tag: "disposition", source: backupSource, focus: 0 }), { _tag: "PressedBack" }).model.flow)
      .toEqual({ _tag: "password", for: { kind: "switch", encryptedSecret: SECRET }, draft: draft("pw", "letters", "done") })
    expect(after(at({ _tag: "disposition", source: bunkerSource, focus: 0 }), { _tag: "PressedBack" }).model.flow)
      .toEqual({ _tag: "bunker", draft: draft(bunkerSource.bunkerUri, "uri", "done") })
    expect(after(at({ _tag: "trust", source: bunkerSource, disposition: "delete", focus: 1 }), { _tag: "PressedBack" }).model.flow)
      .toEqual({ _tag: "disposition", source: bunkerSource, focus: 1 })
    expect(after(at({ _tag: "confirmBackedUp", publicKey: KEY_A, focus: 1 }), { _tag: "PressedBack" }).model.flow)
      .toEqual({ _tag: "confirmDelete", publicKey: KEY_A, focus: 0 })
  })
})

describe("KorriPublished", () => {
  test("a republished model never dismisses Korri's status", () => {
    for (const identity of [problem, switched, ready, working]) {
      const model = at({ _tag: "showSecret", onSlab: false })
      expect(after(model, { _tag: "KorriPublished" }, identity)).toEqual({ model, requests: [] })
    }
  })
  test("clamps the cursor when retired keys go", () => {
    expect(after(at({ _tag: "idle" }, 4), { _tag: "KorriPublished" }, identityOf({ _tag: "Idle" }, [KEY_A])).model.card).toBe(3)
  })
  test("ends a deletion whose key Korri no longer lists", () => {
    const { model } = after(at({ _tag: "confirmBackedUp", publicKey: KEY_B, focus: 1 }, 4), { _tag: "KorriPublished" }, identityOf({ _tag: "Idle" }, [KEY_A]))
    expect(model).toEqual(at({ _tag: "idle" }, 3))
  })
  test("without identity management the page is one card and no flow", () => {
    expect(withoutIdentity(at({ _tag: "secret", draft: draft("x") }, 2), { _tag: "KorriPublished" }).model).toEqual(IDENTITY_START)
    expect(withoutIdentity(IDENTITY_START, { _tag: "KorriPublished" }).model).toBe(IDENTITY_START)
  })
})

describe("flows end to end", () => {
  test("C16: back up", () => {
    const { model, requests } = run(idle, { _tag: "ChoseSaveCard", index: 0 }, ...type("hunter2"), { _tag: "PressedDone" })
    expect(model.flow._tag).toBe("showSecret")
    expect(requests).toEqual([{ _tag: "ExportIdentityBackup", password: "hunter2" }])
  })
  test("C17: switch from a backup", () => {
    const { requests } = run(idle,
      { _tag: "ChoseSaveCard", index: 1 }, ...type("ncryptsec1xyz"), { _tag: "PressedDone" },
      ...type("pw"), { _tag: "PressedDone" },
      { _tag: "ChoseDisposition", disposition: "delete" },
      { _tag: "Answered", yes: true })
    expect(requests).toEqual([{
      _tag: "SwitchIdentityFromBackup", encryptedSecret: "ncryptsec1xyz", password: "pw", disposition: "delete", trustLossConfirmed: true,
    }])
  })
  test("C18: switch to a bunker", () => {
    const { requests } = run(idle,
      { _tag: "ChoseSaveCard", index: 2 }, ...type("abc?relay=wss://r"), { _tag: "PressedDone" },
      { _tag: "ChoseDisposition", disposition: "transfer" },
      { _tag: "Answered", yes: true })
    expect(requests).toEqual([{ _tag: "SwitchIdentityToNip46", bunkerUri: "bunker://abc?relay=wss://r", disposition: "transfer", trustLossConfirmed: true }])
  })
  test("C19: a retired key is deleted only after both questions", () => {
    const one = run(idle, { _tag: "ChoseSaveCard", index: 3 }, { _tag: "Answered", yes: true })
    expect(one.requests).toEqual([])
    const both = run(idle, { _tag: "ChoseSaveCard", index: 3 }, { _tag: "Answered", yes: true }, { _tag: "Answered", yes: true })
    expect(both.requests).toEqual([{ _tag: "DeleteRetiredIdentity", publicKey: KEY_A, backupConfirmed: true }])
  })
})
