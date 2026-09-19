import { describe, expect, test } from "bun:test"
import { readLinuxRuntimeConfig } from "./runtime-config"

const capability = "ab".repeat(32)

describe("readLinuxRuntimeConfig", () => {
  test("consumes both values from the private Linux RPC binding", () => {
    let portReads = 0
    let capabilityReads = 0

    expect(readLinuxRuntimeConfig({
      korridPort() {
        portReads += 1
        return 45231
      },
      korridCapability() {
        capabilityReads += 1
        return capability
      },
    })).toEqual({
      korridPort: 45231,
      korridCapability: capability,
    })
    expect(portReads).toBe(1)
    expect(capabilityReads).toBe(1)
  })

  test("fails closed when the binding is absent or invalid", () => {
    expect(() => readLinuxRuntimeConfig(undefined)).toThrow(
      "Linux did not provide a valid korrid connection.",
    )

    for (const binding of [
      { korridPort: () => 0, korridCapability: () => capability },
      { korridPort: () => 45231, korridCapability: () => "short" },
      { korridPort: () => { throw new Error("secret") }, korridCapability: () => capability },
    ]) {
      expect(() => readLinuxRuntimeConfig(binding)).toThrow(
        "Linux did not provide a valid korrid connection.",
      )
    }
  })
})
