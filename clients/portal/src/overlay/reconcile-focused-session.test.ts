import { describe, expect, test } from "bun:test"
import type { KorridClient } from "../korrid/client"
import { createInMemoryKorridClient } from "../korrid/client"
import {
  createInspectionGeneration,
  reconcileFocusedSession,
} from "./reconcile-focused-session"

const LAUNCH = "0123456789abcdef0123456789abcdef"

function client(
  statuses: Awaited<ReturnType<KorridClient["sessionStatus"]>>[],
): KorridClient {
  const base = createInMemoryKorridClient()
  let index = 0
  return {
    ...base,
    sessionStatus: async () => statuses[Math.min(index++, statuses.length - 1)]!,
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(next => { resolve = next })
  return { promise, resolve }
}

const failure = {
  _tag: "Err" as const,
  payload: { code: "HostUnavailable", message: "starting" },
}
const active = (phase: "running" | "frozen" | "focus-failed") => {
  const session = {
    launchId: LAUNCH,
    gameId: "game",
    phase,
  }
  return {
    _tag: "Ok" as const,
    payload: {
      active: session,
      ...(phase === "running" ? {} : { overlay: session }),
    },
  }
}

describe("focused session reconciliation", () => {
  test("a newer focus inspection wins when status reads complete out of order", async () => {
    const generations = createInspectionGeneration()
    const olderStatus = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
    const newerStatus = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
    const shown: string[] = []
    const older = reconcileFocusedSession({
      korrid: { ...createInMemoryKorridClient(), sessionStatus: () => olderStatus.promise },
      isCurrent: generations.begin(),
      onActive: () => shown.push("older"),
    })
    const newer = reconcileFocusedSession({
      korrid: { ...createInMemoryKorridClient(), sessionStatus: () => newerStatus.promise },
      isCurrent: generations.begin(),
      onActive: () => shown.push("newer"),
    })

    newerStatus.resolve(active("frozen"))
    expect(await newer).toBe("shown")
    olderStatus.resolve(active("frozen"))
    expect(await older).toBe("superseded")
    expect(shown).toEqual(["newer"])
  })

  test("showCatalog and Open Korri invalidate an in-flight focus inspection", async () => {
    for (const source of ["showCatalog", "Open Korri"]) {
      const generations = createInspectionGeneration()
      const status = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
      const shown: string[] = []
      const inspection = reconcileFocusedSession({
        korrid: { ...createInMemoryKorridClient(), sessionStatus: () => status.promise },
        isCurrent: generations.begin(),
        onActive: () => shown.push(source),
      })

      generations.invalidate()
      status.resolve(active("frozen"))
      expect(await inspection).toBe("superseded")
      expect(shown).toEqual([])
    }
  })

  test("recovers a frozen Home session after bounded transient status failures", async () => {
    const shown: string[] = []
    const sleeps: number[] = []

    const outcome = await reconcileFocusedSession({
      korrid: client([failure, failure, active("frozen")]),
      onActive: session => shown.push(session.launchId),
      sleep: async milliseconds => { sleeps.push(milliseconds) },
    })

    expect(outcome).toBe("shown")
    expect(shown).toEqual([LAUNCH])
    expect(sleeps).toEqual([75, 75])
  })

  test("a delayed pre-freeze read cannot mount an overlay for a running launch", async () => {
    const status = deferred<Awaited<ReturnType<KorridClient["sessionStatus"]>>>()
    const shown: string[] = []
    const inspection = reconcileFocusedSession({
      korrid: { ...createInMemoryKorridClient(), sessionStatus: () => status.promise },
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })

    status.resolve(active("running"))
    expect(await inspection).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("a replacement running launch cannot be mounted after the prior freeze is refused", async () => {
    const replacement = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([{
        _tag: "Ok",
        payload: {
          active: {
            launchId: replacement,
            gameId: "replacement",
            phase: "running",
          },
        },
      }]),
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("A exiting before the browser read cannot hand off an overlay", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([{ _tag: "Ok", payload: {} }]),
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("a frozen replacement B without A's overlay intent cannot be mounted", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([{
        _tag: "Ok",
        payload: {
          active: {
            launchId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            gameId: "replacement",
            phase: "frozen",
          },
        },
      }]),
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("an already-focused catalog detects retained portal ownership without a focus event", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([active("focus-failed")]),
      allowFrozen: false,
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("shown")
    expect(shown).toEqual([LAUNCH])
  })

  test("the retained-failure watch never retargets to a replacement launch", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([{
        _tag: "Ok",
        payload: {
          active: {
            launchId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            gameId: "replacement",
            phase: "focus-failed",
          },
          overlay: {
            launchId: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            gameId: "replacement",
            phase: "focus-failed",
          },
        },
      }]),
      allowFrozen: false,
      expectedLaunchId: LAUNCH,
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("the retained-failure watch does not reopen an intentionally selected catalog", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([active("frozen")]),
      allowFrozen: false,
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("startup does not turn an ordinary running session into an overlay", async () => {
    const shown: string[] = []
    expect(await reconcileFocusedSession({
      korrid: client([active("running")]),
      onActive: session => shown.push(session.launchId),
      sleep: async () => {},
    })).toBe("not-active")
    expect(shown).toEqual([])
  })

  test("stops after the bounded retry budget", async () => {
    let calls = 0
    const base = client([failure])
    const korrid: KorridClient = {
      ...base,
      sessionStatus: async timeout => {
        calls += 1
        return base.sessionStatus(timeout)
      },
    }
    expect(await reconcileFocusedSession({
      korrid,
      onActive: () => {},
      attempts: 3,
      sleep: async () => {},
    })).toBe("unavailable")
    expect(calls).toBe(3)
  })
})
