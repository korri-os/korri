/**
 * Account sign-ins (ADR 0083, ADR 0089), against the small Korri (test/korri.ts):
 *
 *  - One card for any entry: its title, its state, Korri's actions, and in
 *    `Code` the code drawn in the "Pleats" style with a badge in its middle
 *    (the logo, else the title's first letter).
 *  - Settings has a row per entry with its state, and the kicker a signed
 *    out entry carries ("needs sign-in") stands under it at rest.
 *  - Setup has a skippable step for each entry with an `offer`, right after
 *    Wi-Fi, with the offer as its heading.
 *  - A launch that waits for a sign-in (`Code` or `Working` with `forGame`)
 *    takes the screen; Back there is Korri's Cancel.
 *  - Where the window cannot give a module 4 px beside or over the rows, the
 *    page is one list with the code first, and nothing is left out.
 *  - Every action is a RunAction with Korri's id.
 */
import { describe, expect, test } from "bun:test"
import { badgeModules, monogram, plateModules, QUIET, qrPath } from "../src/account"
import { breakLines } from "../src/fit"
import { codeSlot, CODE_MODULE_PX, plan } from "../src/plan"
import { Play, play, scenario, SIZES } from "./korri"

const focused = (run: Play) => [...(run.view.listRows ?? []), ...(run.view.listButtons ?? [])].find((entry: any) => entry.focused === true)
const runs = (requests: readonly any[]) => requests.filter(request => request._tag === "RunAction").map(request => request.actionId)
const labels = (rows: readonly any[]) => rows.map(row => row.label)
const codeOf = (name: string) => scenario(name).models[0].accountSignIns[0].state.qr
const grid = (size: number, dark: (x: number, y: number) => boolean) =>
  ({ size, modules: Array.from({ length: size * size }, (_, at) => (dark(at % size, Math.floor(at / size)) ? "1" : "0")).join("") })

describe("the code", () => {
  test("the plate is the largest odd number of modules at most 0.3 of the size, at least 7, and under 9 % of the code", () => {
    expect([25, 33, 37, 41, 57, 65].map(plateModules)).toEqual([7, 9, 11, 11, 17, 19])
    // The smallest code cannot spare 7 x 7 (11 %): no plate, no badge.
    expect(plateModules(21)).toBe(0)
    expect(badgeModules(grid(21, () => false))).toBe(0)
    for (const size of [25, 37, 57, 65, 177]) {
      const badge = plateModules(size) - 2
      // The badge takes under a tenth of the code, well under the contract's fifth.
      expect((badge * badge) / (size * size)).toBeLessThan(0.1)
    }
  })
  test("each vertical run of dark modules is one capsule, offset by the quiet zone", () => {
    // Column 12 dark from row 9 to row 13: the cleared plate (k = 7, rows and columns 9 to 15): no run there.
    const code = grid(25, (x, y) => x === 12 && y >= 9 && y <= 13)
    expect(qrPath(code).match(/M[^M]*a\.39/g) ?? []).toEqual([])
    const outside = grid(21, (x, y) => x === 5 && (y >= 8 && y <= 10))
    const path = qrPath(outside)
    expect(path.startsWith(`M${5 + QUIET + 0.11} ${8 + QUIET + 0.43}a.39 .39 0 0 1 .78 0v2.14a.39 .39 0 0 1-.78 0z`)).toBe(true)
  })
  test("the three finders are rounded squares in the even-odd path, whatever the grid holds there", () => {
    const path = qrPath(grid(21, () => false))
    expect(path.match(/M/g)?.length).toBe(9)
    expect(path).toContain(`M${QUIET + 2} ${QUIET}h3a2 2 0 0 1 2 2`)
    expect(qrPath(grid(21, () => true)).match(/M/g)!.length).toBeGreaterThan(9)
  })
  test("a grid that is not a code draws nothing", () => {
    expect(qrPath({ size: 21, modules: "101" })).toBe("")
    expect(qrPath({ size: 21, modules: "x".repeat(441) })).toBe("")
    expect(qrPath({ size: 3, modules: "110011000" })).toBe("")
    expect(qrPath({ size: 0, modules: "" })).toBe("")
    expect(badgeModules({ size: 3, modules: "110011000" })).toBe(0)
  })
  test("the badge's letter is the title's first, in caps", () => {
    expect(monogram("Steam")).toBe("S")
    expect(monogram("  tailscale")).toBe("T")
    expect(monogram("")).toBe("")
  })
  test("the card sends the code's path, its viewbox with the quiet zone, and the badge", () => {
    const run = play(scenario("account-code"))
    const code = codeOf("account-code")
    expect(run.view.list.panel).toBe("qr")
    expect(run.view.listQrSize).toBe(code.size + 2 * QUIET)
    expect(run.view.listQr).toBe(qrPath(code))
    expect(run.view.list.codeBadge).toBe(plateModules(code.size) - 2)
    expect(run.view.list.codeMark).toBe("S")
    expect(run.view.list.codeLogoShown).toBe(false)
    expect(run.view.list.panelTitle).toBe("Sign in to Steam")
  })
  test("a logo Korri serves takes the badge's letter's place", () => {
    const scene = scenario("account-code")
    const model = structuredClone(scene.models[0])
    model.accountSignIns[0].logo = { kind: "logo", url: "korri-media://logo/steam", width: 256, height: 256 }
    const run = play({ ...scene, models: [model] })
    expect(run.view.list.codeLogoShown).toBe(true)
    expect(run.view.list.codeLogo.$image).toBe("korri-media://logo/steam")
    expect(run.view.list.codeMark).toBe("")
  })
  test("a new code replaces the old one in the next view", () => {
    const first = play({ ...scenario("account-code-renewed"), steps: scenario("account-code-renewed").steps.slice(0, -1) })
    const renewed = play(scenario("account-code-renewed"))
    expect(renewed.view.listQr).not.toBe(first.view.listQr)
    expect(renewed.view.listQr).toBe(qrPath(scenario("account-code-renewed").models[1].accountSignIns[0].state.qr))
  })
  test("a code the page cannot draw says so, and Korri's actions stay", () => {
    const run = play(scenario("account-code-malformed"))
    expect(run.view.listQr).toBe("")
    expect(labels(run.view.listRows)).toContain("Korri sent a sign-in code this page cannot draw.")
    expect(runs(run.press("confirm"))).toEqual(["steam-account:cancel"])
  })
})

describe("settings", () => {
  test("a row for each entry with its state; it opens the entry's card", () => {
    const run = play(scenario("account-rows"))
    const rows = run.view.listRows as any[]
    expect(focused(run)?.focusId).toBe("v:account:@korri:steam")
    expect(focused(run)?.value).toBe("lantern_keeper_1987")
    expect(rows.find(row => row.focusId === "v:account:@korri:tailscale")?.value).toBe("Not signed in")
    run.press("confirm")
    expect(run.view.list.key).toBe("page:account:@korri:steam")
    expect(run.view.list.title).toBe("STEAM")
    expect(run.view.list.sub).toBe("Signed in as lantern_keeper_1987.")
  })
  test("the kicker a signed out entry carries stands under its row at rest", () => {
    const rows = play(scenario("account-rows")).view.listRows as any[]
    const at = rows.findIndex(row => row.focusId === "v:account:@korri:tailscale")
    expect(rows[at + 1]).toMatchObject({ kind: "note", label: "Tailscale needs you to sign in again" })
    const card = play(scenario("account-expired"))
    expect(card.view.list.sub).toBe("Tailscale needs you to sign in again")
  })
  test("a problem says so at rest; a sign-in under way says it is signing in", () => {
    const rows = play(scenario("account-rows-busy")).view.listRows as any[]
    expect(rows.find(row => row.focusId === "v:account:@korri:steam")?.value).toBe("Needs attention")
    expect(labels(rows)).toContain("Steam sign-in did not finish")
    expect(rows.find(row => row.focusId === "v:account:@korri:tailscale")?.value).toBe("Signing in")
  })
  test("no account rows when Korri publishes none", () => {
    const run = play({ ...scenario("account-rows"), models: [scenario("settings").models[0]] })
    expect((run.view.listRows as any[]).some(row => String(row.focusId).startsWith("v:account:"))).toBe(false)
  })
  test("each state's first action is Korri's RunAction", () => {
    for (const [name, id] of [
      ["account-signed-out", "account-sign-in:sign-in:@korri:tailscale"],
      ["account-expired", "account-sign-in:sign-in:@korri:tailscale"],
      ["account-starting", "account-sign-in:cancel:@korri:tailscale"],
      ["account-code", "steam-account:cancel"],
      ["account-code-tailscale", "account-sign-in:cancel:@korri:tailscale"],
      ["account-signed-in", "steam-account:sign-out"],
      ["account-problem", "account-sign-in:retry:@korri:tailscale"],
      ["account-refused", "steam-account:use-client-runner"],
    ] as const) {
      const run = play(scenario(name))
      expect(run.view.list.key.startsWith("page:account:"), name).toBe(true)
      expect(runs(run.press("confirm")), name).toEqual([id])
    }
  })
  test("a card with nothing to press has no target and Back closes it", () => {
    const run = play(scenario("account-checking"))
    expect(labels(run.view.listRows)).toEqual(["Checking your Tailscale sign-in"])
    run.press("back")
    expect(run.view.list.key).toBe("page:settings")
  })
  test("Back during a sign-in is its Cancel; on a signed-in card, Back closes the card", () => {
    const code = play(scenario("account-code"))
    expect(runs(code.press("back"))).toEqual(["steam-account:cancel"])
    const signedIn = play(scenario("account-signed-in"))
    expect(runs(signedIn.press("back"))).toEqual([])
    expect(signedIn.view.list.key).toBe("page:settings")
  })
})

describe("a launch waits for the sign-in", () => {
  test("the code takes the screen with the game's name, and Back cancels", () => {
    const run = play(scenario("account-for-game"))
    expect(run.view.list.key).toBe("page:account:@korri:steam")
    expect(run.view.list.sub).toBe("Ember Road needs your Steam sign-in.")
    expect(run.view.list.panel).toBe("qr")
    expect(focused(run)?.label).toBe("Cancel")
    expect(runs(run.press("back"))).toEqual(["steam-account:cancel"])
  })
  test("while Korri signs in, the card says so and the game it is for", () => {
    const run = play(scenario("account-working"))
    expect(run.view.list.key).toBe("page:account:@korri:steam")
    expect(run.view.list.sub).toBe("Ember Road starts when you are signed in.")
    expect(labels(run.view.listRows)).toEqual(["Signing in to Steam"])
  })
  test("a code asked for from Settings does not take the screen by itself", () => {
    const run = new Play(scenario("account-code").models[0])
    expect(run.view.list?.key ?? "").not.toBe("page:account:@korri:steam")
  })
})

describe("setup", () => {
  test("an offered sign-in is a step right after Wi-Fi, headed by its offer", () => {
    const run = play(scenario("first-boot-account"))
    expect(run.view.list.title).toBe("PRIVATE NETWORK: REACH YOUR DEVICES FROM ANYWHERE")
    expect(run.view.list.step).toBe(3)
    expect((run.view.listButtons as any[]).map(button => button.label)).toEqual(["Sign in", "Skip for now"])
    expect(runs(run.press("confirm"))).toEqual(["account-sign-in:sign-in:@korri:tailscale"])
  })
  test("Skip passes the step to the names", () => {
    const run = play(scenario("first-boot-account"))
    run.press("right", "confirm")
    expect(run.view.list.title).toBe("NAMES")
  })
  test("each offered entry has its own step, in Korri's order, and its kicker", () => {
    const run = play(scenario("first-boot-accounts"))
    expect(run.view.list.title).toBe("PRIVATE NETWORK: REACH YOUR DEVICES FROM ANYWHERE")
    expect(run.view.list.step).toBe(4)
    expect(run.view.list.sub).toBe("Tailscale needs you to sign in again")
    run.press("back")
    expect(run.view.list.title).toBe("STEAM: PLAY THE GAMES IN YOUR STEAM LIBRARY")
    expect(focused(run)?.label).toBe("Next")
  })
  test("the code in setup: the card's code, with Skip beside Korri's Cancel", () => {
    const run = play(scenario("first-boot-account-code"))
    expect(run.view.list.panel).toBe("qr")
    expect(run.view.list.codeMark).toBe("T")
    expect((run.view.listButtons as any[]).map(button => button.label)).toEqual(["Cancel", "Skip for now"])
  })
  test("signed in: Next leads", () => {
    const run = play(scenario("first-boot-account-done"))
    expect(focused(run)?.label).toBe("Next")
    expect((run.view.listButtons as any[]).map(button => button.label)).toEqual(["Next", "Sign out"])
  })
  test("an entry without an offer is not a step", () => {
    const scene = scenario("first-boot-account")
    const model = structuredClone(scene.models[0])
    delete model.accountSignIns[0].offer
    expect(play({ ...scene, models: [model] }).view.list.title).toBe("NAMES")
  })
})

describe("the smallest window", () => {
  test("where the plan gives a module under 4 px, the page is one list, the code first, and nothing is left out", () => {
    const wide = play(scenario("account-for-game"), 1280, 720)
    const small = play(scenario("account-for-game"), 320, 240)
    expect(wide.view.list.codeFirst).toBe(false)
    expect(small.view.list.codeFirst).toBe(true)
    expect(small.view.listButtons).toEqual([])
    const rows = small.view.listRows as any[]
    expect(rows[1]).toMatchObject({ kind: "code", focusId: "a:code", focused: true })
    // Every word and every action of the wide card is a row of the list.
    const words = [wide.view.list.title, wide.view.list.sub, wide.view.list.panelTitle, ...wide.view.listPanelLines, ...labels(wide.view.listRows), ...labels(wide.view.listButtons)]
    for (const word of words) expect(labels(rows).map(label => label.toUpperCase()), word).toContain(word.toUpperCase())
    expect(small.view.listQr).toBe(wide.view.listQr)
  })
  test("from the code, Down reaches Korri's actions and Up the title", () => {
    const run = play(scenario("account-for-game"), 320, 240)
    run.press("down")
    expect(focused(run)?.label).toBe("Cancel")
    expect(runs(run.press("confirm"))).toEqual(["steam-account:cancel"])
    run.press("up", "up")
    expect(focused(run)?.focusId).toBe("a:title")
    expect(focused(run)?.note).toBe("Ember Road needs your Steam sign-in.")
  })
  test("focus on Cancel survives a resize from the wide card to the one list", () => {
    const run = play(scenario("account-for-game"), 1280, 720)
    expect(focused(run)?.label).toBe("Cancel")
    run.deliver({ _tag: "Resized", width: 320, height: 240 })
    expect(focused(run)?.label).toBe("Cancel")
  })
  test("at every proven size, a module of the code is 4 px or more, or the page is one list", () => {
    expect(play(scenario("account-code"), 320, 240).view.list.codeFirst).toBe(true)
    for (const [width, height] of SIZES) {
      for (const name of ["account-code", "account-code-largest", "account-for-game", "first-boot-account-code"]) {
        const run = play(scenario(name), width, height)
        const modules = run.view.listQrSize
        if (run.view.list.codeFirst) continue
        const p = plan(width, height)
        const sub = run.view.list.sub === "" ? 0 : breakLines(run.view.list.sub, p.fact, Math.min(width - 2 * p.padX, p.body * 34)).length
        expect(codeSlot(p, sub, (run.view.listButtons as any[]).length) / modules, `${name} ${width}x${height}`).toBeGreaterThanOrEqual(CODE_MODULE_PX)
      }
    }
  })
})
