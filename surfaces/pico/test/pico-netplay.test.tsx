/**
 * Netplay in Pico, on the receiving device: an invite's routes beside stream
 * play, and every state Korri publishes for it — copy progress with Cancel,
 * Connecting, Paused while the holder has left, Reconnecting, Ended, each
 * Problem, and Settings' plugin review before an install.
 *
 * Pico invents none of it. Every word and action is Korri's; these tests
 * render what Korri publishes and read what the person sees and what Korri
 * receives back.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel, SurfaceStatus } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel } from "../src/fixtures/fixture-host"
import {
  inviteModel,
  netplayCopyFailedStatus,
  netplayCopyInvite,
  netplayCopyingStatus,
  netplayEndedStatus,
  netplayInstallingStatus,
  netplayInstallInvite,
  netplayLongInvites,
  netplayLostStatus,
  netplayMixedInvites,
  netplayNoRouteStatus,
  netplayNoSpaceStatus,
  netplayPausedStatus,
  netplayReadyInvite,
  netplayReconnectingStatus,
  netplayReviewLongStatus,
  netplayReviewStatus,
  netplayVersionStatus,
} from "../src/fixtures/named-states"
import * as homePart from "../src/pages/PicoHome.page.part"
import { PicoSurface } from "../src/PicoSurface"

afterEach(() => cleanup())

const playingNow = () => screen.getByRole("region", { name: "Playing now" })
const surface = (model: SurfaceModel) => {
  const host = createFixtureHost()
  const view = render(<PicoSurface host={host} model={model} />)
  return { host, view, actions: () => host.calls.filter(call => call.startsWith("action:")) }
}
const status = (value: SurfaceStatus): SurfaceModel => ({ ...fixtureModel, status: value })

describe("an invite's routes", () => {
  test("each route is a button in Korri's order and words, and runs its own id", () => {
    for (const invite of [netplayReadyInvite, netplayCopyInvite, netplayInstallInvite]) {
      const { actions } = surface(inviteModel([invite]))
      const buttons = within(playingNow()).getAllByRole("button")
      expect(buttons.map(button => button.textContent)).toEqual(invite.actions.map(action => action.label))
      for (const button of buttons) fireEvent.click(button)
      expect(actions()).toEqual(invite.actions.map(action => `action:${action.id}`))
      cleanup()
    }
  })

  test("Play here, Copy and play here and Install … and play here, as Korri words them", () => {
    surface(inviteModel(netplayMixedInvites))
    const labels = within(playingNow()).getAllByRole("button").map(button => button.textContent)
    expect(labels).toContain("Play here")
    expect(labels).toContain("Copy and play here (612 MB)")
    expect(labels).toContain("Install Snes9x 2010 and play here")
    // The invite with stream play only keeps exactly its two actions.
    expect(labels.filter(label => label === "Dismiss").length).toBe(4)
    expect(labels.filter(label => label === "Play on stream").length).toBe(4)
  })

  test("a long route is never cut: its button holds every word and may wrap", () => {
    surface(inviteModel(netplayLongInvites))
    const long = within(playingNow()).getByRole("button", { name: "Install Beetle PSX HW (Mednafen PlayStation) and play here" })
    expect(long.textContent).toBe("Install Beetle PSX HW (Mednafen PlayStation) and play here")
    expect(long.closest(".pico-invite-row-actions")).not.toBeNull()
  })
})

describe("the states while netplay runs", () => {
  test("copying: Korri's words, a real bar from its bytes, the percent, and Cancel", () => {
    const { actions } = surface(status(netplayCopyingStatus))
    expect(screen.getByRole("heading", { name: "Copying Turtles in Time from RG35XX SP" })).toBeTruthy()
    expect(document.body.textContent).toContain("42% of 1 MB")
    expect(screen.getByRole("progressbar")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    expect(actions()).toEqual(["action:netplay:cancel"])
  })

  test("connecting: Korri's words and Cancel", () => {
    render(<homePart.NetplayConnecting />)
    expect(screen.getByRole("heading", { name: "Connecting to RG35XX SP" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy()
  })

  test("paused while the holder has left: Leave runs Korri's id; Back stays the host's", () => {
    const { host, actions } = surface(status(netplayPausedStatus))
    expect(screen.getByRole("heading", { name: "Paused on RG35XX SP" })).toBeTruthy()
    act(() => host.press("back"))
    expect(host.calls).toEqual([])
    fireEvent.click(screen.getByRole("button", { name: "Leave" }))
    expect(actions()).toEqual(["action:netplay:leave:mine"])
  })

  test("reconnecting: nothing to press and no B hint, because the host takes no Back", () => {
    surface(status(netplayReconnectingStatus))
    expect(screen.getByRole("heading", { name: "Reconnecting to RG35XX SP" })).toBeTruthy()
    expect(screen.queryAllByRole("button").filter(button => button.closest(".pico-launch-stage") !== null)).toEqual([])
    expect(document.body.textContent).not.toContain("BACK")
  })

  test("ended, by the holder or with the device lost: Korri's reason, and OK dismisses", () => {
    for (const ended of [netplayEndedStatus, netplayLostStatus]) {
      const { host } = surface(status(ended))
      expect(screen.getByText(ended.reason)).toBeTruthy()
      fireEvent.click(screen.getByRole("button", { name: "OK" }))
      expect(host.calls).toEqual(["dismiss"])
      cleanup()
    }
  })

  test("every problem is a red card with Korri's reason, and OK dismisses it", () => {
    for (const problem of [netplayNoRouteStatus, netplayVersionStatus, netplayCopyFailedStatus, netplayNoSpaceStatus]) {
      const { host } = surface(status(problem))
      expect(screen.getByRole("heading", { name: problem.kicker })).toBeTruthy()
      expect(screen.getByText(problem.reason)).toBeTruthy()
      fireEvent.click(screen.getByRole("button", { name: "OK" }))
      expect(host.calls).toEqual(["dismiss"])
      cleanup()
    }
  })
})

describe("the plugin review before an install", () => {
  test("a one-line report reads as a sentence, with Korri's answers after it", () => {
    const { actions } = surface(status(netplayReviewStatus))
    expect(screen.getByRole("heading", { name: "Install Snes9x 2010?" })).toBeTruthy()
    expect(screen.getByText(netplayReviewStatus.detail)).toBeTruthy()
    expect(document.querySelectorAll(".pico-notice-line").length).toBe(0)
    fireEvent.click(screen.getByRole("button", { name: "Install and play here" }))
    expect(actions()).toEqual(["action:netplay:install"])
  })

  test("a long report: one stop per line, the cursor on the first, the answers above it", () => {
    const { actions } = surface(status(netplayReviewLongStatus))
    const lines = [...document.querySelectorAll<HTMLElement>(".pico-notice-line")]
    expect(lines.map(line => line.textContent)).toEqual(netplayReviewLongStatus.detail.split("\n"))
    expect(lines.every(line => line.tabIndex === 0)).toBe(true)
    expect(document.activeElement).toBe(lines[0] ?? null)
    // The answers come before the report, so a report that scrolls never
    // carries them off the screen.
    const answers = screen.getByRole("button", { name: "Install and play here" })
    expect(answers.compareDocumentPosition(lines[0]!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    // A line is a statement: a press on it answers nothing.
    fireEvent.click(lines[0]!)
    expect(actions()).toEqual([])
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }))
    expect(actions()).toEqual(["action:netplay:cancel"])
  })

  test("a report never takes the cursor from a control the person holds", () => {
    const host = createFixtureHost()
    render(<div><button type="button">Held</button></div>)
    const held = screen.getByRole("button", { name: "Held" })
    held.focus()
    render(<PicoSurface host={host} model={status(netplayReviewLongStatus)} />)
    expect(document.activeElement).toBe(held)
  })

  test("installing has nothing to press and no B hint", () => {
    surface(status(netplayInstallingStatus))
    expect(screen.getByRole("heading", { name: "Installing Snes9x 2010" })).toBeTruthy()
    expect(document.body.textContent).not.toContain("BACK")
  })
})

describe("the words", () => {
  // CONTEXT.md: netplay and stream play are the two kinds of shared play.
  test("no netplay screen says join, guest, co-op or player two", () => {
    const words = [
      ...netplayMixedInvites.flatMap(invite => invite.actions.map(action => action.label)),
      ...[netplayCopyingStatus, netplayPausedStatus, netplayReconnectingStatus, netplayEndedStatus, netplayLostStatus,
        netplayNoRouteStatus, netplayVersionStatus, netplayCopyFailedStatus, netplayNoSpaceStatus, netplayReviewStatus]
        .flatMap(value => [value.kicker, "reason" in value ? value.reason : "", "detail" in value ? value.detail ?? "" : ""]),
    ].join(" ").toLowerCase()
    for (const avoided of ["join", "guest", "co-op", "player two", "player 2"]) expect(words).not.toContain(avoided)
  })
})
