/**
 * The Steam sign-in (ADR 0083), against the small Korri (test/korri.ts):
 *
 *  - Settings shows a Steam row while Korri publishes `steamAccount`, and
 *    opens its page; no row without it.
 *  - First boot has a Steam step after the existing steps, before the end,
 *    that can always be passed.
 *  - A launch that waits for the sign-in (`Code` or `Working` with
 *    `forGame`) takes the screen; Back there is Korri's Cancel.
 *  - Every action is a RunAction with Korri's id.
 *  - The QR code is Korri's module grid, drawn as runs of dark modules
 *    inside a quiet zone of four.
 */
import { describe, expect, test } from "bun:test"
import { QUIET, qrPath } from "../src/steam"
import { Play, play, scenario } from "./korri"

const focused = (run: Play) => [...(run.view.listRows ?? []), ...(run.view.listButtons ?? [])].find((entry: any) => entry.focused === true)
const runs = (requests: readonly any[]) => requests.filter(request => request._tag === "RunAction").map(request => request.actionId)

describe("the QR code", () => {
  test("each run of dark modules is one rectangle, offset by the quiet zone", () => {
    expect(qrPath({ size: 3, modules: "110011000" })).toBe(`M${QUIET} ${QUIET}h2v1h-2zM${1 + QUIET} ${1 + QUIET}h2v1h-2z`)
  })
  test("a grid that is not size × size of 0 and 1 draws nothing", () => {
    expect(qrPath({ size: 2, modules: "101" })).toBe("")
    expect(qrPath({ size: 2, modules: "10x1" })).toBe("")
    expect(qrPath({ size: 0, modules: "" })).toBe("")
  })
  test("the page sends the code's path and its viewbox with the quiet zone", () => {
    const run = play(scenario("steam-code"))
    const code = scenario("steam-code").models[0].steamAccount.qr
    expect(run.view.list.panel).toBe("qr")
    expect(run.view.listQrSize).toBe(code.size + 2 * QUIET)
    expect(run.view.listQr).toBe(qrPath(code))
    expect(run.view.listQr.length).toBeGreaterThan(0)
  })
})

describe("settings", () => {
  test("a Steam row says who is signed in and opens the page", () => {
    const run = play(scenario("steam-row"))
    expect(focused(run)?.focusId).toBe("v:steam")
    expect(focused(run)?.value).toBe("lantern_keeper_1987")
    run.press("confirm")
    expect(run.view.list.key).toBe("page:steam")
    expect(run.view.list.sub).toBe("Signed in as lantern_keeper_1987.")
  })
  test("no Steam row when no engine runner is installed", () => {
    const run = play({ ...scenario("steam-row"), models: [scenario("settings").models[0]] })
    expect((run.view.listRows as any[]).some(row => row.focusId === "v:steam")).toBe(false)
  })
  test("each state's action is Korri's RunAction", () => {
    for (const [name, id] of [
      ["steam-signed-out", "steam-account:sign-in"],
      ["steam-code", "steam-account:cancel"],
      ["steam-signed-in", "steam-account:sign-out"],
      ["steam-problem", "steam-account:retry"],
      ["steam-refused", "steam-account:use-client-runner"],
    ] as const) {
      const run = play(scenario(name))
      expect(run.view.list.key).toBe("page:steam")
      expect(runs(run.press("confirm"))).toEqual([id])
    }
  })
  test("Back during a sign-in is its Cancel; on a signed-in page, Back closes the page", () => {
    const code = play(scenario("steam-code"))
    expect(runs(code.press("back"))).toEqual(["steam-account:cancel"])
    const signedIn = play(scenario("steam-signed-in"))
    expect(runs(signedIn.press("back"))).toEqual([])
    expect(signedIn.view.list.key).toBe("page:settings")
  })
})

describe("a launch waits for the sign-in", () => {
  test("the code takes the screen with the game's name, and Back cancels", () => {
    const run = play(scenario("steam-for-game"))
    expect(run.view.list.key).toBe("page:steam")
    expect(run.view.list.sub).toBe("Ember Road needs your Steam sign-in.")
    expect(run.view.list.panel).toBe("qr")
    expect(focused(run)?.label).toBe("Cancel")
    expect(runs(run.press("back"))).toEqual(["steam-account:cancel"])
  })
  test("while Korri signs in, the page says so and the game it is for", () => {
    const run = play(scenario("steam-working"))
    expect(run.view.list.key).toBe("page:steam")
    expect(run.view.list.sub).toBe("Ember Road starts when you are signed in.")
    expect((run.view.listRows as any[]).map(row => row.label)).toEqual(["Signing in to Steam"])
  })
  test("a code asked for from Settings does not take the screen by itself", () => {
    const run = new Play(scenario("steam-code").models[0])
    expect(run.view.list?.key ?? "").not.toBe("page:steam")
  })
})

describe("first boot", () => {
  test("the Steam step comes after the existing steps and before the end", () => {
    const run = play(scenario("first-boot-steam"))
    expect(run.view.list.title).toBe("STEAM")
    expect(run.view.list.step).toBe(run.view.list.steps - 1)
    expect((run.view.listButtons as any[]).map(button => button.label)).toEqual(["Sign in", "Skip for now"])
    expect(runs(run.press("confirm"))).toEqual(["steam-account:sign-in"])
  })
  test("Skip passes the step to the end", () => {
    const run = play(scenario("first-boot-steam"))
    run.press("right", "confirm")
    expect(run.view.list.title).toBe("THE HOUSE IS READY")
  })
  test("signed in: Next leads", () => {
    const run = play(scenario("first-boot-steam-done"))
    expect(focused(run)?.label).toBe("Next")
    expect((run.view.listButtons as any[]).map(button => button.label)).toEqual(["Next", "Sign out"])
  })
  test("no Steam step without an engine runner", () => {
    const steps = play(scenario("first-boot-ready"))
    expect(steps.view.list.title).toBe("THE HOUSE IS READY")
  })
})
