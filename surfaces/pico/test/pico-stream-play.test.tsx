/**
 * Stream play in Pico: invites (a notice each, and PLAYING NOW on home), the
 * host's connecting and failure screens, a session paused or ended on the
 * device that holds it, and who plays on player 1's stream.
 *
 * The rules are driven through `update` with plain values; the screens are
 * rendered and read for what the person sees and what Korri receives.
 */
import { afterEach, describe, expect, test } from "bun:test"
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react"
import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { createFixtureHost, fixtureModel, fixtureOverlay } from "../src/fixtures/fixture-host"
import {
  dimmedInvite,
  fixtureInvite,
  fixtureInvites,
  fixtureSecondInvite,
  inviteModel,
  streamEndedStatus,
  streamPausedStatus,
  streamPlayersOverlay,
} from "../src/fixtures/named-states"
import * as homePart from "../src/pages/PicoHome.page.part"
import * as overlayPart from "../src/pages/PicoOverlay.page.part"
import * as libraryPart from "../src/pages/PicoLibrary.page.part"
import * as onboardingPart from "../src/pages/PicoOnboarding.page.part"
import { PicoSurface } from "../src/PicoSurface"
import { PICO_INVITE_NOTICE_MS } from "../src/pico-invite-view"
import { picoOverlayViewFrom } from "../src/pico-overlay-view"
import { picoScreenViewFromModel } from "../src/pico-screen-view"
import type { PicoMessage } from "../src/state/messages"
import { initialNavigation, type PicoNavigation } from "../src/state/navigation"
import type { PicoRequest } from "../src/state/requests"
import { inviteNotices } from "../src/state/shown"
import { update } from "../src/state/update"
import * as noticesPart from "../src/ui/organisms/PicoInviteNotices.organism.part"
import * as playingNowPart from "../src/ui/organisms/PicoPlayingNow.organism.part"

afterEach(() => cleanup())

function storyFrom(start: PicoNavigation, korri: SurfaceModel, ...messages: PicoMessage[]) {
  let nav = start
  const requests: PicoRequest[] = []
  for (const message of messages) {
    const step = update(nav, message, korri)
    nav = step.model
    requests.push(...step.requests)
  }
  return { nav, requests, notices: inviteNotices(nav, korri).map(notice => notice.id) }
}
const story = (korri: SurfaceModel, ...messages: PicoMessage[]) => storyFrom(initialNavigation(), korri, ...messages)

const published = { _tag: "KorriPublished" } as const
const back = { _tag: "PressedBack" } as const
const system = { _tag: "PressedSystem" } as const
const waits = (requests: readonly PicoRequest[]) => requests.filter(request => request._tag === "Wait")

describe("invite notices", () => {
  test("each new invite gets one notice and its own wait; the wait ends it, the invite stays", () => {
    const one = story(inviteModel([fixtureInvite]), published)
    expect(one.notices).toEqual([fixtureInvite.id])
    expect(waits(one.requests)).toEqual([{ _tag: "Wait", ms: PICO_INVITE_NOTICE_MS, token: 0 }])

    const two = storyFrom(one.nav, inviteModel([fixtureInvite, fixtureSecondInvite]), published)
    expect(two.notices).toEqual([fixtureSecondInvite.id, fixtureInvite.id])
    expect(waits(two.requests)).toEqual([{ _tag: "Wait", ms: PICO_INVITE_NOTICE_MS, token: 1 }])

    const done = storyFrom(two.nav, inviteModel([fixtureInvite, fixtureSecondInvite]), { _tag: "Waited", token: 0 })
    expect(done.notices).toEqual([fixtureSecondInvite.id])
  })

  test("several at once: one notice each, newest first", () => {
    const { notices, requests } = story(inviteModel(fixtureInvites), published)
    expect(notices).toEqual([...fixtureInvites].reverse().map(invite => invite.id))
    expect(waits(requests).length).toBe(3)
  })

  test("an invite is announced once: gone and back, it does not announce again", () => {
    const first = story(inviteModel([fixtureInvite]), published, { _tag: "Waited", token: 0 })
    const gone = storyFrom(first.nav, fixtureModel, published)
    const again = storyFrom(gone.nav, inviteModel([fixtureInvite]), published)
    expect(again.notices).toEqual([])
    expect(waits(again.requests)).toEqual([])
  })

  test("a notice goes when its invite goes", () => {
    const { nav } = story(inviteModel([fixtureInvite]), published)
    expect(storyFrom(nav, fixtureModel, published).nav.invites.showing).toEqual([])
  })

  test("a press ends the notices on screen; a focus move does not", () => {
    const { nav } = story(inviteModel([fixtureInvite]), published)
    expect(storyFrom(nav, inviteModel([fixtureInvite]), { _tag: "MovedFocus" }).notices).toEqual([fixtureInvite.id])
    expect(storyFrom(nav, inviteModel([fixtureInvite]), { _tag: "PressedScreen" }).notices).toEqual([])
  })

  test("shown over home and Find only; an invite that came during Settings is announced on the way back", () => {
    const find = story(inviteModel([fixtureInvite]), { _tag: "PressedOptions" }, published)
    expect(find.notices).toEqual([fixtureInvite.id])

    const settings = story(fixtureModel, system)
    const during = storyFrom(settings.nav, inviteModel([fixtureInvite]), published)
    expect(during.notices).toEqual([])
    expect(waits(during.requests)).toEqual([])
    const after = storyFrom(during.nav, inviteModel([fixtureInvite]), system)
    expect(after.notices).toEqual([fixtureInvite.id])
  })

  test("not over a launch, a failure, or a paused stream", () => {
    for (const status of [
      { _tag: "Busy", kicker: "Connecting…" },
      { _tag: "Problem", kicker: "No", reason: "No.", canRetry: false },
      streamPausedStatus,
    ] as const) {
      expect(story({ ...inviteModel([fixtureInvite]), status }, published).notices).toEqual([])
    }
  })

  test("a notice has nothing to focus and lets a pointer through", () => {
    const { container } = render(<noticesPart.SeveralInvites />)
    const stack = container.querySelector(".pico-invite-notices")
    expect(stack?.getAttribute("role")).toBe("status")
    expect(stack?.querySelectorAll("button, a, input, [tabindex]").length).toBe(0)
    expect(stack?.textContent).toContain("Simon is playing")
    expect(stack?.textContent).toContain("Turtles in Time")
  })
})

describe("PLAYING NOW", () => {
  test("runs exactly the invite's action ids, only while Korri publishes them enabled and the device is idle", () => {
    const korri = inviteModel([fixtureInvite])
    const press = (actionId: string, model: SurfaceModel = korri) =>
      story(model, { _tag: "PressedInviteAction", actionId }).requests.filter(request => request._tag === "RunAction")
    expect(press(`${fixtureInvite.id}:stream`)).toEqual([{ _tag: "RunAction", actionId: `${fixtureInvite.id}:stream` }])
    expect(press(`${fixtureInvite.id}:dismiss`)).toEqual([{ _tag: "RunAction", actionId: `${fixtureInvite.id}:dismiss` }])
    expect(press("invite-gone:stream")).toEqual([])
    expect(press(`${fixtureInvite.id}:stream`, inviteModel([dimmedInvite]))).toEqual([])
    expect(press(`${fixtureInvite.id}:stream`, { ...korri, status: { _tag: "Busy", kicker: "Connecting…" } })).toEqual([])
  })

  test("the last row going gives the cursor back to the chosen cart on the shelf", () => {
    const { nav } = story(fixtureModel, { _tag: "LostInviteFocus" })
    expect(nav.home.focusOnReturn).toBe("Cart")
    const grid = story(fixtureModel, { _tag: "PressedMenu" }, { _tag: "LostInviteFocus" })
    expect(grid.nav.home.focusOnReturn).toBe("None")
  })

  test("lists who plays what, where, with the cover, and Korri's labels as buttons", () => {
    render(<homePart.PlayingNowThree />)
    const entry = screen.getByRole("region", { name: "Playing now" })
    expect(within(entry).getByText("Turtles in Time")).toBeTruthy()
    expect(within(entry).getByText("Simon is playing on Odin 2 Portal")).toBeTruthy()
    expect(within(entry).getByText("3 GAMES")).toBeTruthy()
    expect(within(entry).getAllByRole("button", { name: "Play on stream" }).length).toBe(3)
    expect(within(entry).getAllByRole("button", { name: "Dismiss" }).length).toBe(3)
    expect(entry.querySelectorAll("canvas").length).toBe(3)
  })

  test("is not on home with no invite, nor in a host without shared play", () => {
    render(<homePart.Ready />)
    expect(screen.queryByText("PLAYING NOW")).toBeNull()
    expect(render(<playingNowPart.NoInvites />).container.childElementCount).toBe(0)
  })

  test("a dimmed route stays, and does nothing", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={inviteModel([dimmedInvite])} />)
    const route = screen.getByRole("button", { name: "Play on stream" })
    expect(route.hasAttribute("disabled")).toBe(true)
    fireEvent.click(route)
    expect(host.calls.filter(call => call.startsWith("action:"))).toEqual([])
  })

  test("end to end: the notice and the row appear, and Play on stream reaches Korri unchanged", () => {
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={inviteModel([fixtureInvite])} />)
    const notice = screen.getByRole("status")
    expect(notice.textContent).toBe("Simon is playingTurtles in Timeon Odin 2 Portal")
    fireEvent.click(within(screen.getByRole("region", { name: "Playing now" })).getByRole("button", { name: "Play on stream" }))
    expect(host.calls).toContain(`action:${fixtureInvite.id}:stream`)
  })

  test("the shelf keeps its cursor when an invite arrives", () => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={fixtureModel} />)
    act(() => screen.getByRole("button", { name: /Tetris/ }).focus())
    // Entering the shelf puts the cursor on the chosen cart; that is the cart to keep.
    const before = document.activeElement
    expect(before?.tagName).toBe("BUTTON")
    view.rerender(<PicoSurface host={host} model={inviteModel([fixtureInvite])} />)
    expect(document.activeElement === before).toBe(true)
    expect(before?.isConnected).toBe(true)
  })

  test("the cursor moves to the next row when Korri takes its row away", () => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={inviteModel(fixtureInvites)} />)
    const entry = () => screen.getByRole("region", { name: "Playing now" })
    const firstDismiss = within(entry()).getAllByRole("button", { name: "Dismiss" })[0]!
    act(() => firstDismiss.focus())
    view.rerender(<PicoSurface host={host} model={inviteModel(fixtureInvites.slice(1))} />)
    expect(document.activeElement?.textContent).toBe("Play on stream")
    expect(entry().contains(document.activeElement)).toBe(true)
  })
})

describe("the cursor after the last invite", () => {
  test("goes back to the chosen cart on the shelf", () => {
    const host = createFixtureHost()
    const view = render(<PicoSurface host={host} model={inviteModel([fixtureInvite])} />)
    act(() => screen.getByRole("button", { name: "Dismiss" }).focus())
    view.rerender(<PicoSurface host={host} model={fixtureModel} />)
    expect(screen.queryByRole("region", { name: "Playing now" })).toBeNull()
    expect(screen.getByRole("list", { name: "Shelf" }).contains(document.activeElement)).toBe(true)
    expect(document.activeElement?.getAttribute("aria-label") ?? document.activeElement?.textContent).toContain("Celeste Classic")
  })
})

describe("paused and ended on the device that holds the session", () => {
  test("paused names the device and carries exactly Korri's actions; Back does nothing", () => {
    const paused = { ...fixtureModel, status: streamPausedStatus }
    const view = picoScreenViewFromModel(paused)
    expect(view._tag === "Paused" && [view.kicker, view.actions]).toEqual([streamPausedStatus.kicker, streamPausedStatus.actions])
    expect(story(paused, back).requests).toEqual([])
    expect(story(paused, { _tag: "PressedStatusAction", actionId: "stream:leave" }).requests)
      .toEqual([{ _tag: "RunAction", actionId: "stream:leave" }])
    expect(story(paused, { _tag: "PressedStatusAction", actionId: "stream:end" }).requests).toEqual([])
  })

  test("ended states Korri's reason; OK and Back acknowledge it through dismiss", () => {
    const ended = { ...fixtureModel, status: streamEndedStatus }
    expect(picoScreenViewFromModel(ended)).toEqual({ _tag: "Ended", kicker: streamEndedStatus.kicker, reason: streamEndedStatus.reason })
    expect(story(ended, back).requests).toEqual([{ _tag: "DismissProblem" }])
    const host = createFixtureHost()
    render(<PicoSurface host={host} model={ended} />)
    expect(screen.getByText("Turtles in Time ended on Odin 2 Portal.")).toBeTruthy()
    fireEvent.click(screen.getByRole("button", { name: "OK" }))
    expect(host.calls).toEqual(["dismiss"])
  })

  test("the paused screen offers Korri's action and no meter", () => {
    render(<homePart.StreamPaused />)
    expect(screen.getByRole("heading", { name: "Paused on Odin 2 Portal" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "Leave" })).toBeTruthy()
    expect(screen.queryByRole("progressbar")).toBeNull()
  })
})

describe("the gameplay overlay", () => {
  test("names who plays on the stream above the controls, and nothing when nobody does", () => {
    expect(picoOverlayViewFrom(streamPlayersOverlay, fixtureModel.status).streamPlayers).toEqual(["Ana", "Living room"])
    expect(picoOverlayViewFrom({ ...fixtureOverlay, streamPlayers: [] }, fixtureModel.status).streamPlayers).toBeUndefined()
    render(<overlayPart.StreamPlayers />)
    const players = screen.getByRole("region", { name: "On the stream" })
    expect(within(players).getAllByRole("listitem").map(item => item.textContent)).toEqual(["Ana", "Living room"])
    expect(players.querySelectorAll("button, [tabindex]").length).toBe(0)
  })
})

describe("the words", () => {
  // CONTEXT.md: the feature is stream play. "Join" names the device-join design.
  test("no stream play screen says Join", () => {
    const states = [
      homePart.InviteNotice, homePart.InviteNotices, homePart.PlayingNowOne, homePart.PlayingNowThree,
      homePart.StreamConnecting, homePart.StreamFailed, homePart.StreamPaused, homePart.StreamEnded,
      libraryPart.InviteNotice, overlayPart.StreamPlayers, onboardingPart.PersonName, onboardingPart.PersonNameFilled,
    ]
    for (const State of states) {
      const { container, unmount } = render(<State />)
      expect(container.textContent ?? "").not.toMatch(/join/i)
      unmount()
    }
  })
})
