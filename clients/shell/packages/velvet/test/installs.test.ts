/**
 * Installs (ADR 0092, #553), against the small Korri (test/korri.ts):
 *
 *  - On a game only another device holds, "Install and play" stands after
 *    Play and never takes its place or its focus; "Install for later" is in
 *    the game's More. Korri never picks either.
 *  - Why an install cannot happen stands under the buttons; the choice stays,
 *    dimmed when pressing again cannot help.
 *  - While it installs: a bar, the words of where it stands, and Installs.
 *  - Korri's stream-meanwhile question: the way out (wait) first and on
 *    Back; one stream answer a device when several hold the game.
 *  - The list: reachable from the menu, Settings and the detail; Install
 *    next, Move up, Move down and Cancel install by where an entry stands; a
 *    failed one's reason at rest, Try again and Throw away.
 *  - The list and its sheet keep their focus at every size.
 */
import { describe, expect, test } from "bun:test"
import { Play, play, scenario, SIZES, targets } from "./korri"
import { blocked, inLine, left, takes, yielded } from "../src/words"

const model = (name: string, at = 0) => scenario(name).models[at]
const focus = (run: Play) => targets(run.view).find(entry => entry.focused === true)
const labels = (run: Play) => (run.view.detailButtons as any[]).map(button => button.label)
const own = (requests: readonly any[]) => requests.filter(request => request._tag !== "MoveCatalog" && request._tag !== "FocusGame")
const toPeer = ["down", "down", "down", "down", "confirm"]
const atPeer = (name: string, at = 0) => {
  const run = new Play(model(name, at))
  run.press(...toPeer)
  return run
}

describe("the detail of a game only another device holds", () => {
  test("Install and play stands after Play; Play keeps the focus and still streams", () => {
    const run = atPeer("install-offered")
    expect(run.view.detail.mark.key).toBe("glass-garden")
    expect(labels(run)).toEqual(["Play", "Install and play", "More"])
    expect(focus(run)?.focusId).toBe("d:play")
    expect(own(run.press("confirm"))).toEqual([{ _tag: "LaunchGame", gameId: "glass-garden" }])
  })
  test("Install and play asks Korri to install and start it; nothing is sent before the press", () => {
    const run = atPeer("install-offered")
    expect(own(run.requests)).toEqual([])
    expect(own(run.press("right", "confirm"))).toEqual([{ _tag: "InstallGame", gameId: "glass-garden", play: true }])
  })
  test("Install for later is the first row of More, and options opens it too", () => {
    const run = atPeer("install-offered")
    run.press("options")
    expect(run.view.sheetRows[0].label).toBe("Install for later")
    expect(own(run.press("confirm"))).toEqual([{ _tag: "InstallGame", gameId: "glass-garden", play: false }])
    expect(run.view.sheet.shown).toBe(false)
  })
  test("a game this device holds has no install choice", () => {
    const run = new Play(model("detail"))
    run.press("down", "left", "confirm")
    expect(labels(run).some(label => label.startsWith("Install"))).toBe(false)
  })
  test("a plugin can run it: the same choices, and the note names the plugin", () => {
    const run = atPeer("install-plugin")
    expect(labels(run)).toEqual(["Play", "Install and play", "More"])
    expect(run.view.detail.note).toContain("mGBA plugin")
  })
  test("the plugin's review is Korri's Confirm, with its own answers", () => {
    const run = play(scenario("install-plugin-review"))
    expect(run.view.curtain.kind).toBe("ask")
    expect((run.view.curtainButtons as any[]).map(button => button.label)).toEqual(["Not now", "Install mGBA"])
  })
  test("while Korri checks, the choice says so and does nothing", () => {
    const run = play(scenario("install-checking"))
    expect(labels(run)).toEqual(["Play", "Checking"])
    expect(run.view.detailButtons[1].dim).toBe(true)
    expect(own(run.press("confirm"))).toEqual([])
  })
})

describe("why not", () => {
  test("no runner and not allowed: the choice stays, dimmed, and the reason stands under it", () => {
    for (const [name, reason] of [["install-no-runner", "noRunner"], ["install-not-allowed", "notAllowed"]] as const) {
      const run = atPeer(name)
      expect(labels(run)).toEqual(["Play", "Install and play"])
      expect(run.view.detailButtons[1].dim).toBe(true)
      expect(run.view.detail.note).toBe(blocked(reason, undefined))
      expect(own(run.press("right", "confirm"))).toEqual([])
    }
  })
  test("not enough space says how much it needs; pressing again checks again", () => {
    const run = atPeer("install-no-space")
    expect(run.view.detail.note).toContain("4.7 GB")
    expect(own(run.press("right", "confirm"))).toEqual([{ _tag: "InstallGame", gameId: "glass-garden", play: true }])
  })
  test("no device answers: pressing again checks again", () => {
    const run = atPeer("install-no-source")
    expect(run.view.detail.note).toBe(blocked("noSource", undefined))
    expect(run.view.detailButtons[1].dim).toBe(false)
  })
})

describe("while it installs", () => {
  test("a bar, how far, the time left, and that it starts by itself", () => {
    const run = atPeer("install-running")
    expect(run.view.detail.progress).toBe(true)
    expect(run.view.detail.share).toBeCloseTo(1.2 / 4.7, 3)
    expect(run.view.detail.note).toBe("Installing on this device: 1.2 GB of 4.7 GB, about 7 min left. It starts when it is installed.")
    expect(labels(run)).toEqual(["Play", "Installs"])
  })
  test("installing for later: Install and play stays, to start it when it is done", () => {
    const run = atPeer("install-running-later")
    expect(labels(run)).toEqual(["Play", "Install and play", "Installs"])
    expect(own(run.press("right", "confirm"))).toEqual([{ _tag: "InstallGame", gameId: "glass-garden", play: true }])
  })
  test("waiting in line: no bar, and its place", () => {
    const run = atPeer("install-waiting")
    expect(run.view.detail.progress).toBe(false)
    expect(run.view.detail.note).toBe("Waiting to install: next in line.")
  })
  test("Installs opens the list", () => {
    const run = atPeer("install-running")
    run.press("right", "confirm")
    expect(run.view.list.key).toBe("page:installs")
    run.press("back")
    expect(run.view.page).toBe("detail")
  })
})

describe("an install that yields to play (#556)", () => {
  test("paused: the detail says why and when it goes on, and no time left that only grows", () => {
    const run = atPeer("install-paused")
    expect(run.view.detail.progress).toBe(true)
    expect(run.view.detail.note).toBe("Installing on this device: 1.2 GB of 4.7 GB. Paused while a game runs on this device. It goes on when the game ends. It starts when it is installed.")
  })
  test("slowed: the time left stays, and why", () => {
    const run = atPeer("install-slowed")
    expect(run.view.detail.note).toBe("Installing on this device: 2.6 GB of 4.7 GB, about 11 min left. Slowed: the devices that hold it are playing or low on battery.")
  })
  test("the list: Paused or Slowed as its value, the reason under its bar at rest", () => {
    const paused = play(scenario("installs-paused"))
    const rows = (paused.view.listRows as any[]).slice(0, 3).map(row => [row.kind, row.label, row.value])
    expect(rows).toEqual([
      ["row", "Vesper", "Paused"],
      ["meter", "3.1 GB of 8.4 GB", ""],
      ["note", yielded("paused", "netplay"), ""],
    ])
    const slowed = play(scenario("installs-slowed"))
    expect(slowed.view.listRows[0].value).toBe("Slowed")
    expect(slowed.view.listRows[1].label).toBe("3.1 GB of 8.4 GB, about 33 min left")
    expect(slowed.view.listRows[2].label).toBe("Slowed while this device plays a stream.")
  })
  test("its sheet and the menu say it is paused", () => {
    const sheet = play(scenario("installs-paused-sheet"))
    expect(sheet.view.sheetRows[1].label).toBe(`Installing: 3.1 GB of 8.4 GB. ${yielded("paused", "sourcesBusy")}`)
    const menu = new Play(model("installs-paused"))
    menu.press("menu")
    expect((menu.view.sheetRows as any[]).find(row => row.label === "Installs").value).toBe("Paused, 36%")
  })
  test("every pace and reason has its own words", () => {
    const all = (["slowed", "paused"] as const).flatMap(pace => (["game", "netplay", "stream", "sourcesBusy"] as const).map(reason => yielded(pace, reason)))
    expect(new Set(all).size).toBe(8)
    for (const words of all.slice(4)) expect(words).toContain("It goes on when")
  })
})

describe("stream meanwhile", () => {
  test("the curtain asks; the way out is first, focused, and Back gives the same answer", () => {
    const run = play(scenario("install-meanwhile"))
    expect(run.view.curtain.kind).toBe("ask")
    expect(run.view.curtainKicker).toEqual(["ABOUT 7 MINUTES TO INSTALL"])
    expect((run.view.curtainButtons as any[]).map(button => `${button.label}${button.focused ? "*" : ""}`)).toEqual(["Wait for the install*", "Stream while it installs"])
    expect(own(run.press("back"))).toEqual([{ _tag: "AnswerStreamMeanwhile", installId: "dev:glass", stream: false }])
  })
  test("streaming answers without a place when one device holds it", () => {
    const run = play(scenario("install-meanwhile"))
    expect(own(run.press("right", "confirm"))).toEqual([{ _tag: "AnswerStreamMeanwhile", installId: "dev:glass", stream: true }])
  })
  test("several devices hold it: one answer a device, none chosen", () => {
    const run = play(scenario("install-meanwhile-places"))
    expect((run.view.curtainButtons as any[]).map(button => button.label)).toEqual(["Wait for the install", "Stream from zao", "Stream from aka"])
    const along = run.view.curtain.stack ? "down" : "right"
    expect(own(run.press(along, along, "confirm"))).toEqual([{ _tag: "AnswerStreamMeanwhile", installId: "dev:glass", stream: true, launchLocationId: "aka" }])
  })
  test("no question over the gameplay overlay", () => {
    const base = model("install-meanwhile", 1)
    const run = new Play({ ...base, presentation: model("overlay").presentation })
    expect(run.view.curtain.shown).toBe(false)
  })
})

describe("the list of installs", () => {
  test("from the menu, from Settings, and from the detail", () => {
    const menu = new Play(model("installs"))
    menu.press("menu")
    expect((menu.view.sheetRows as any[]).map(row => row.label)).toContain("Installs")
    menu.press("down", "confirm")
    expect(menu.view.list.key).toBe("page:installs")
    const settings = play(scenario("installs-settings"))
    expect(focus(settings)?.focusId).toBe("v:installs")
    settings.press("confirm")
    expect(settings.view.list.key).toBe("page:installs")
  })
  test("a host without installs has no row and no page", () => {
    const run = new Play(model("home"))
    run.press("menu")
    expect((run.view.sheetRows as any[]).map(row => row.label)).not.toContain("Installs")
  })
  test("what runs with its bar, what waits by its place, what failed with its reason at rest", () => {
    const run = play(scenario("installs"))
    const rows = (run.view.listRows as any[]).map(row => [row.kind, row.label, row.value])
    expect(rows).toEqual([
      ["row", "Vesper", "Installing"],
      ["meter", "3.1 GB of 8.4 GB, about 33 min left", ""],
      ["row", "Glass Garden", "Next in line"],
      ["row", "mGBA core", "2nd in line"],
      ["row", "The Long Winter of the Clockmaker's Apprentice", "3rd in line"],
      ["row", "Antidisestablishmentarianism", "Failed"],
      ["problem", expect.stringContaining("stopped answering"), ""],
    ])
  })
  test("nothing installing: it says so, and Back is a button", () => {
    const run = play(scenario("installs-empty"))
    expect(run.view.listRows[0].kind).toBe("note")
    expect(run.view.listButtons[0].label).toBe("Back")
  })
  test("one that waits: Install next, Move up, Move down; the sheet stays for the next press", () => {
    const run = play(scenario("installs-sheet"))
    expect(run.view.sheet.title).toBe("mGBA core")
    expect(run.view.sheetRows[0]).toMatchObject({ kind: "head", label: "mGBA core" })
    expect(own(run.press("confirm"))).toEqual([{ _tag: "MoveInstall", installId: "artifact:@korri:mgba/core", index: 0 }])
    expect(run.view.sheet.shown).toBe(true)
    expect(own(run.press("down", "confirm"))).toEqual([{ _tag: "MoveInstall", installId: "artifact:@korri:mgba/core", index: 1 }])
    expect(own(run.press("down", "confirm"))).toEqual([{ _tag: "MoveInstall", installId: "artifact:@korri:mgba/core", index: 3 }])
  })
  test("the next one cannot move up; the last that waits cannot move down", () => {
    const next = play(scenario("installs-sheet-next"))
    const dim = (run: Play) => (run.view.sheetRows as any[]).filter(row => row.dim).map(row => row.label)
    expect(dim(next)).toEqual(["Install next", "Move up"])
    expect(own(next.press("confirm"))).toEqual([])
    const last = new Play(model("installs"))
    last.press("menu", "down", "confirm", "down", "down", "down", "confirm")
    expect(last.view.sheet.title).toBe("The Long Winter of the Clockmaker's Apprentice")
    expect(dim(last)).toEqual(["Move down"])
  })
  test("cancel asks first; the way out keeps it", () => {
    const run = play(scenario("installs-cancel"))
    expect((run.view.curtainButtons as any[]).map(button => button.label)).toEqual(["Keep it", "Cancel install"])
    expect(own(run.press("confirm"))).toEqual([])
    expect(run.view.sheet.shown).toBe(true)
    run.press("down", "down", "down", "confirm")
    expect(own(run.press("right", "confirm"))).toEqual([{ _tag: "CancelInstall", installId: "artifact:@korri:mgba/core" }])
    expect(run.view.sheet.shown).toBe(false)
  })
  test("the one that runs can be cancelled; a failed one tried again or thrown away", () => {
    const running = play(scenario("installs-sheet-running"))
    expect((running.view.sheetRows as any[]).filter(row => row.focusId !== "").map(row => row.label)).toEqual(["Cancel install"])
    const failed = play(scenario("installs-sheet-failed"))
    expect((failed.view.sheetRows as any[]).map(row => row.label)).toEqual(["Antidisestablishmentarianism", expect.stringContaining("stopped answering"), "Try again", "Throw away"])
    expect(own(failed.press("confirm"))).toEqual([{ _tag: "InstallGame", gameId: "antidisestablishmentarianism", play: false }])
    const away = play(scenario("installs-sheet-failed"))
    away.press("down", "confirm", "right")
    expect(own(away.press("confirm"))).toEqual([{ _tag: "CancelInstall", installId: "dev:anti" }])
  })
  test("an entry that leaves the list closes nothing by surprise: its sheet says so", () => {
    const run = play(scenario("installs-sheet"))
    run.swap({ ...model("installs"), installs: [] })
    expect(run.view.sheetRows[0].label).toBe("This install is no longer in the list.")
  })
  test("at every size every entry and every sheet row is reachable by presses", () => {
    for (const [width, height] of SIZES) {
      const run = new Play(model("installs-many"), width, height)
      run.press("menu", "down", "confirm")
      const seen = new Set<string>()
      for (let i = 0; i < 14; i++) {
        seen.add(focus(run)?.focusId)
        run.press("down")
      }
      expect(seen.size).toBe(12)
      run.press("up", "up", "up", "confirm")
      expect(run.view.sheet.shown).toBe(true)
      const rows = new Set<string>()
      for (let i = 0; i < 5; i++) {
        rows.add(focus(run)?.focusId)
        run.press("down")
      }
      expect([...rows]).toEqual(["i:next", "i:up", "i:down", "i:cancel"])
    }
  })
})

describe("the words", () => {
  test("time left and how long it takes", () => {
    expect(left(30)).toBe("under a minute left")
    expect(left(412)).toBe("about 7 min left")
    expect(left(4800)).toBe("about 1 h 20 min left")
    expect(takes(412)).toBe("About 7 minutes to install")
    expect(takes(60)).toBe("About 1 minute to install")
    expect(takes(5400)).toBe("About 1.5 hours to install")
    expect(takes(undefined)).toBe("This takes a while to install")
  })
  test("a place in line", () => {
    expect([1, 2, 3, 4, 11, 12, 13, 21, 22, 101].map(inLine)).toEqual(["Next in line", "2nd in line", "3rd in line", "4th in line", "11th in line", "12th in line", "13th in line", "21st in line", "22nd in line", "101st in line"])
  })
})
