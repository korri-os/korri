import { describe, expect, it } from "bun:test"
import { waitForKorrid } from "./wait-for-korrid"

const unreachable = { _tag: "Err" as const, payload: { code: "BrainUnreachable", message: "refused" } }
const ok = { _tag: "Ok" as const, payload: { version: "test" } }

function clock() {
  let time = 0
  return {
    now: () => time,
    sleep: async (ms: number) => { time += ms },
  }
}

describe("waitForKorrid", () => {
  it("returns once korrid answers, polling while it is unreachable", async () => {
    const outcomes = [unreachable, unreachable, ok]
    let calls = 0
    const korrid = { health: async () => outcomes[calls++] ?? ok }
    expect(await waitForKorrid(korrid, { ...clock(), intervalMs: 250, deadlineMs: 10_000 })).toBe("answered")
    expect(calls).toBe(3)
  })

  it("stops waiting on any answer that is not unreachable", async () => {
    let calls = 0
    const korrid = {
      health: async () => {
        calls++
        return { _tag: "Err" as const, payload: { code: "Internal", message: "answered with an error" } }
      },
    }
    expect(await waitForKorrid(korrid, clock())).toBe("answered")
    expect(calls).toBe(1)
  })

  it("gives up at the deadline so the surface still mounts as before", async () => {
    let calls = 0
    const korrid = { health: async () => { calls++; return unreachable } }
    expect(await waitForKorrid(korrid, { ...clock(), intervalMs: 250, deadlineMs: 1_000 })).toBe("deadline")
    expect(calls).toBe(4)
  })
})
