import { describe, expect, test } from "bun:test"
import { loadLinuxRuntimeConfig } from "./runtime-config"

const capability = "ab".repeat(32)

describe("loadLinuxRuntimeConfig", () => {
  test("loads the capability-bound local korrid endpoint and selected surface", async () => {
    let input: RequestInfo | URL | undefined
    let init: RequestInit | undefined
    const fetcher = async (nextInput: string, nextInit: RequestInit) => {
      input = nextInput
      init = nextInit
      return new Response(JSON.stringify({
        korridPort: 45231,
        korridCapability: capability,
        surfaceId: "pico",
      }), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
    }

    await expect(loadLinuxRuntimeConfig(fetcher)).resolves.toEqual({
      korridPort: 45231,
      korridCapability: capability,
      surfaceId: "pico",
    })
    expect(input).toBe("/runtime.json")
    expect(init).toMatchObject({ cache: "no-store", credentials: "same-origin" })
  })

  test("fails closed when the endpoint or payload is invalid", async () => {
    const unavailable = async () => new Response("no", { status: 503 })
    await expect(loadLinuxRuntimeConfig(unavailable)).rejects.toThrow("503")

    for (const payload of [
      {},
      { korridPort: 0, korridCapability: capability },
      { korridPort: 45231, korridCapability: "short" },
      { korridPort: 45231, korridCapability: capability, surfaceId: "" },
    ]) {
      const malformed = async () => new Response(JSON.stringify(payload), {
        status: 200,
        headers: { "content-type": "application/json" },
      })
      await expect(loadLinuxRuntimeConfig(malformed)).rejects.toThrow(
        "invalid Linux runtime configuration",
      )
    }
  })
})
