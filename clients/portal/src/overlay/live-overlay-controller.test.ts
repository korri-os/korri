import { describe, expect, test } from "bun:test"
import {
  SessionControlFailureReason,
  SessionFreezerState,
  SessionStopPhase,
  type SessionControls,
} from "@contracts/generated/korrid"
import type { KorridClient } from "../korrid/client"
import { createInMemoryKorridClient } from "../korrid/client"
import { createLiveOverlayController } from "./live-overlay-controller"
import {
  OVERLAY_END_CONTROL_ID,
  OVERLAY_OPEN_KORRI_CONTROL_ID,
  OVERLAY_RETURN_CONTROL_ID,
} from "./overlay-model"

const LAUNCH = "0123456789abcdef0123456789abcdef"
const listed: SessionControls = {
  launchId: LAUNCH,
  title: "Wario Land 4",
  groups: [{
    id: "@korri:mgba",
    label: "mGBA",
    controls: [{
      id: "@korri:mgba/open-menu",
      label: "Open RetroArch menu",
      enabled: true,
      destructive: false,
      dismissOnSuccess: false,
      interaction: { kind: "command" },
    }],
  }],
}

function platform() {
  const calls: string[] = []
  return {
    calls,
    value: {
      openKorri: () => calls.push("open-korri"),
      sessionEnded: () => calls.push("ended"),
      sessionChanged: () => calls.push("changed"),
    },
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(next => { resolve = next })
  return { promise, resolve }
}

function client(overrides: Partial<KorridClient> = {}): KorridClient {
  return Object.assign(createInMemoryKorridClient({
    activeSession: { launchId: LAUNCH, gameId: "wario", phase: "frozen" },
    sessionControls: listed,
  }), overrides)
}

describe("live gameplay overlay controller", () => {
  test("loads the exact frozen session and only korrid-listed actions", async () => {
    const p = platform()
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client(),
      platform: p.value,
    })

    await controller.refresh()

    expect(controller.model().presentation).toMatchObject({
      kind: "gameplay-overlay",
      title: "Wario Land 4",
      controls: [
        { id: OVERLAY_RETURN_CONTROL_ID },
        { id: OVERLAY_END_CONTROL_ID },
        { id: OVERLAY_OPEN_KORRI_CONTROL_ID },
      ],
      groups: [{ controls: [{ id: "@korri:mgba/open-menu" }] }],
    })
    expect(JSON.stringify(controller.model())).not.toContain("launchId")
  })

  test("an older refresh cannot overwrite a newer inspection", async () => {
    const first = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
    const second = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
    let calls = 0
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionStatus: () => (++calls === 1 ? first.promise : second.promise),
      }),
      platform: platform().value,
    })

    const older = controller.refresh()
    const newer = controller.refresh()
    second.resolve({
      _tag: "Ok",
      payload: { active: { launchId: LAUNCH, gameId: "wario", phase: "frozen" } },
    })
    await newer
    expect(controller.model().status).toEqual({ _tag: "Browsing" })

    first.resolve({
      _tag: "Ok",
      payload: { active: { launchId: LAUNCH, gameId: "wario", phase: "running" } },
    })
    await older
    expect(controller.model().status).toEqual({ _tag: "Browsing" })
  })

  test("an exact session with no plugin controls shows only core controls", async () => {
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionControls: async () => ({
          _tag: "Ok",
          payload: { launchId: LAUNCH, title: "Neverball", groups: [] },
        }),
      }),
      platform: platform().value,
    })

    await controller.refresh()

    expect(controller.model().status).toEqual({ _tag: "Browsing" })
    const presentation = controller.model().presentation
    expect(presentation.kind).toBe("gameplay-overlay")
    if (presentation.kind === "gameplay-overlay") {
      expect(presentation.groups).toEqual([])
      expect(presentation.controls.map(control => control.id)).toEqual([
        OVERLAY_RETURN_CONTROL_ID,
        OVERLAY_END_CONTROL_ID,
        OVERLAY_OPEN_KORRI_CONTROL_ID,
      ])
    }
  })

  test("return, end, and Open Korri keep the exact launch id", async () => {
    const calls: string[] = []
    const base = client()
    const p = platform()
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionThaw: async launchId => {
          calls.push(`thaw:${launchId}`)
          return { _tag: "Ok", payload: {
            launchId, state: SessionFreezerState.Running, changed: true,
          } }
        },
        sessionStop: async launchId => {
          calls.push(`stop:${launchId}`)
          return { _tag: "Ok", payload: { phase: SessionStopPhase.Stopped } }
        },
        sessionControls: base.sessionControls,
      }),
      platform: p.value,
    })

    await controller.invoke(OVERLAY_RETURN_CONTROL_ID)
    await controller.invoke(OVERLAY_END_CONTROL_ID)
    await controller.invoke(OVERLAY_OPEN_KORRI_CONTROL_ID)

    expect(calls).toEqual([`thaw:${LAUNCH}`, `stop:${LAUNCH}`])
    expect(p.calls).toEqual(["ended", "open-korri"])
  })

  test("selected remote replacement is terminal stale for poll, Return, and End", async () => {
    const replaced = {
      _tag: "Err" as const,
      payload: {
        code: "SelectedRemoteSessionReplaced",
        message: "the selected peer now runs another launch",
      },
    }

    for (const operation of ["poll", "return", "end"] as const) {
      const p = platform()
      const controller = createLiveOverlayController({
        launchId: LAUNCH,
        korrid: client({
          sessionStatus: async () => replaced,
          sessionThaw: async () => replaced,
          sessionStop: async () => replaced,
        }),
        platform: p.value,
      })

      if (operation === "poll") await controller.refresh()
      else await controller.invoke(
        operation === "return" ? OVERLAY_RETURN_CONTROL_ID : OVERLAY_END_CONTROL_ID,
      )

      expect(p.calls).toEqual(["changed"])
      expect(controller.model().status).toEqual({
        _tag: "Problem",
        kicker: "Session changed",
        reason: "That game is no longer running. Reload the session before acting.",
        canRetry: false,
      })
    }
  })

  test("pending End keeps the exact overlay open until completion is confirmed", async () => {
    const p = platform()
    const calls: string[] = []
    let completed = false
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionStop: async launchId => {
          calls.push(launchId ?? "missing")
          return { _tag: "Ok", payload: { phase: SessionStopPhase.Pending } }
        },
        sessionStatus: async () => completed
          ? {
              _tag: "Err",
              payload: { code: "SessionCompleted", message: "stopped" },
            }
          : {
              _tag: "Ok",
              payload: {
                active: { launchId: LAUNCH, gameId: "wario", phase: "stopping" },
              },
            },
      }),
      platform: p.value,
    })

    await controller.invoke(OVERLAY_END_CONTROL_ID)

    expect(calls).toEqual([LAUNCH])
    expect(p.calls).toEqual([])
    expect(controller.model().presentation.kind).toBe("gameplay-overlay")
    expect(controller.model().status).toEqual({
      _tag: "Problem",
      kicker: "Game is still stopping",
      reason: "Korri has not confirmed that this exact game stopped.",
      canRetry: true,
    })

    await controller.refresh()
    expect(p.calls).toEqual([])
    expect(controller.model().status).toMatchObject({ kicker: "Game is still stopping" })
    completed = true
    await controller.refresh()
    expect(p.calls).toEqual(["ended"])
  })

  test("focus failure keeps Return and the overlay notice available", async () => {
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionThaw: async () => ({
          _tag: "Err",
          payload: { code: "HostFocusFailed", message: "sway refused focus" },
        }),
      }),
      platform: platform().value,
    })

    await controller.invoke(OVERLAY_RETURN_CONTROL_ID)

    expect(controller.model().status).toEqual({
      _tag: "Problem",
      kicker: "Could not return",
      reason: "The game is running, but Korri could not bring its window forward. Return is still available.",
      canRetry: true,
    })
    const presentation = controller.model().presentation
    expect(presentation.kind).toBe("gameplay-overlay")
    if (presentation.kind === "gameplay-overlay") {
      expect(presentation.controls[0]).toMatchObject({
        id: OVERLAY_RETURN_CONTROL_ID,
        enabled: true,
      })
    }
  })

  test("a retained focus failure mounts the notice with Return available", async () => {
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionStatus: async () => ({
          _tag: "Ok",
          payload: {
            active: { launchId: LAUNCH, gameId: "wario", phase: "focus-failed" },
          },
        }),
      }),
      platform: platform().value,
    })

    await controller.refresh()

    expect(controller.model().status).toMatchObject({
      _tag: "Problem",
      kicker: "Could not return",
      canRetry: true,
    })
    const presentation = controller.model().presentation
    expect(presentation.kind).toBe("gameplay-overlay")
    if (presentation.kind === "gameplay-overlay") {
      expect(presentation.controls[0]).toMatchObject({
        id: OVERLAY_RETURN_CONTROL_ID,
        enabled: true,
      })
    }
  })

  test("Retry repeats the exact failed thaw and focus operation", async () => {
    const calls: string[] = []
    let attempt = 0
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionThaw: async launchId => {
          calls.push(launchId)
          attempt += 1
          return attempt === 1
            ? { _tag: "Err", payload: { code: "HostFocusFailed", message: "focus" } }
            : { _tag: "Ok", payload: {
                launchId, state: SessionFreezerState.Running, changed: false,
              } }
        },
      }),
      platform: platform().value,
    })

    await controller.invoke(OVERLAY_RETURN_CONTROL_ID)
    await controller.retry()

    expect(calls).toEqual([LAUNCH, LAUNCH])
  })

  test("background polling preserves a refused action notice and its exact Retry", async () => {
    const calls: Array<[string, string]> = []
    let attempt = 0
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        invokeSessionControl: async (launchId, controlId) => {
          calls.push([launchId, controlId])
          attempt += 1
          return attempt === 1
            ? { _tag: "Err", payload: {
                reason: SessionControlFailureReason.Unavailable,
                message: "Runner did not answer",
              } }
            : { _tag: "Ok", payload: { launchId } }
        },
      }),
      platform: platform().value,
    })
    await controller.refresh()

    await controller.invoke("@korri:mgba/open-menu")
    const refused = controller.model().status

    await controller.refresh()

    expect(controller.model().status).toEqual(refused)
    await controller.retry()

    expect(calls).toEqual([
      [LAUNCH, "@korri:mgba/open-menu"],
      [LAUNCH, "@korri:mgba/open-menu"],
    ])
  })

  test("a completed destructive plugin action ends the overlay without another thaw", async () => {
    const calls: string[] = []
    const p = platform()
    const quitControls: SessionControls = {
      ...listed,
      groups: [{
        ...listed.groups[0]!,
        controls: [{
          ...listed.groups[0]!.controls[0]!,
          id: "@korri:mgba/quit",
          destructive: true,
          dismissOnSuccess: true,
        }],
      }],
    }
    let invoked = false
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionControls: async () => ({ _tag: "Ok", payload: quitControls }),
        invokeSessionControl: async launchId => {
          invoked = true
          return {
            _tag: "Ok",
            payload: { launchId },
          }
        },
        sessionStatus: async () => invoked
          ? {
              _tag: "Err",
              payload: { code: "SessionCompleted", message: "completed" },
            }
          : {
              _tag: "Ok",
              payload: {
                active: { launchId: LAUNCH, gameId: "wario", phase: "frozen" },
              },
            },
        sessionThaw: async launchId => {
          calls.push(launchId)
          return {
            _tag: "Ok",
            payload: { launchId, state: SessionFreezerState.Running, changed: false },
          }
        },
      }),
      platform: p.value,
    })
    await controller.refresh()

    await controller.invoke("@korri:mgba/quit")

    expect(calls).toEqual([])
    expect(p.calls).toEqual(["ended"])
  })

  test("a later observed completion closes an unanswered action notice", async () => {
    let completed = false
    const p = platform()
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        sessionStatus: async () => completed
          ? {
              _tag: "Err",
              payload: { code: "SessionCompleted", message: "completed" },
            }
          : {
              _tag: "Ok",
              payload: {
                active: { launchId: LAUNCH, gameId: "wario", phase: "frozen" },
              },
            },
        invokeSessionControl: async () => ({
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.Unavailable,
            message: "Runner did not answer",
          },
        }),
      }),
      platform: p.value,
    })
    await controller.refresh()
    await controller.invoke("@korri:mgba/open-menu")
    completed = true

    await controller.refresh()

    expect(p.calls).toEqual(["ended"])
  })

  test("an undeclared control is removed instead of retained disabled", async () => {
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        invokeSessionControl: async () => ({
          _tag: "Err",
          payload: {
            reason: SessionControlFailureReason.UnknownControl,
            message: "not declared",
          },
        }),
      }),
      platform: platform().value,
    })
    await controller.refresh()

    await controller.invoke("@korri:mgba/open-menu")
    const refused = controller.model().status

    await controller.refresh()

    expect(controller.model().status).toEqual(refused)
    const presentation = controller.model().presentation
    expect(presentation.kind).toBe("gameplay-overlay")
    if (presentation.kind === "gameplay-overlay") {
      expect(presentation.groups).toEqual([])
    }
    expect(controller.model().status).toMatchObject({
      _tag: "Problem",
      canRetry: false,
    })
  })

  test("refused and unanswered listed actions name a reason and stay open", async () => {
    for (const [reason, message, expected] of [
      [SessionControlFailureReason.Disabled, "Menu disabled by runner", "Menu disabled by runner"],
      [SessionControlFailureReason.Unavailable, "Runner did not answer", "Runner did not answer"],
    ] as const) {
      const controller = createLiveOverlayController({
        launchId: LAUNCH,
        korrid: client({
          invokeSessionControl: async () => ({
            _tag: "Err",
            payload: { reason, message },
          }),
        }),
        platform: platform().value,
      })
      await controller.refresh()

      await controller.invoke("@korri:mgba/open-menu")

      expect(controller.model().status).toMatchObject({
        _tag: "Problem",
        kicker: "Action refused",
        reason: expected,
      })
    }
  })

  test("stale action refusal never retargets to the replacement launch", async () => {
    const calls: string[] = []
    const p = platform()
    const controller = createLiveOverlayController({
      launchId: LAUNCH,
      korrid: client({
        invokeSessionControl: async launchId => {
          calls.push(launchId)
          return {
            _tag: "Err",
            payload: {
              reason: SessionControlFailureReason.StaleSession,
              message: "replacement exists",
            },
          }
        },
      }),
      platform: p.value,
    })
    await controller.refresh()

    await controller.invoke("@korri:mgba/open-menu")

    await controller.retry()

    expect(calls).toEqual([LAUNCH])
    expect(p.calls).toEqual(["changed"])
    expect(controller.model().status).toMatchObject({
      _tag: "Problem",
      kicker: "Session changed",
    })
    const presentation = controller.model().presentation
    expect(presentation.kind).toBe("gameplay-overlay")
    if (presentation.kind === "gameplay-overlay") {
      expect(presentation.groups).toEqual([])
    }
  })
})
