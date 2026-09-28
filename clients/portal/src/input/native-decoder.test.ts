import { describe, expect, it } from "bun:test"
import { decodeNativeInputMessage, type NativeInputMessage } from "./native-decoder"

const INPUT = { kind: "input", deviceId: "pad", class: "gamepad", type: 1, code: 304, value: 1, timestamp: 0 } satisfies NativeInputMessage
const DEVICE = { kind: "device-added", device: { deviceId: "pad", class: "gamepad", name: "Pad", capabilities: [], axes: [] } } satisfies NativeInputMessage

describe("Rust native input treaty runtime decoder", () => {
  it("keeps legacy names and accepts only correlated bounded lifecycle identities", () => {
    for (const message of [INPUT, DEVICE, { kind: "device-removed", deviceId: "pad" },
      { kind: "initialization-complete", generation: "1" }, { kind: "retired", generation: "1" }, { kind: "device-state-complete", deviceId: "pad" },
      { kind: "suspend", generation: "1", requestId: "2" }, { kind: "resume", generation: "1", requestId: "2" }] satisfies NativeInputMessage[]) {
      expect(decodeNativeInputMessage(message)).toEqual(message)
    }
  })
  it.each([
    null, [], 1, "input", {}, { ...INPUT, value: NaN }, { ...INPUT, timestamp: Infinity },
    { ...INPUT, code: 768 }, { ...INPUT, code: -1 }, { ...INPUT, code: 304.5 },
    { ...INPUT, type: 3, code: 64 }, { ...INPUT, type: 32 }, { ...INPUT, value: 3 },
    { ...INPUT, deviceId: "x".repeat(1025) }, { ...INPUT, class: "controller" },
    { ...INPUT, authorization: "irrelevant" }, { ...INPUT, type: 3, value: 2147483648 },
    { ...DEVICE, device: { ...DEVICE.device, axes: Array.from({ length: 65 }, (_, code) => ({ code, minimum: 0, maximum: 255 })) } },
    { ...DEVICE, device: { ...DEVICE.device, axes: [{ code: 0, minimum: 0, maximum: 255 }, { code: 0, minimum: 0, maximum: 255 }] } },
    { ...DEVICE, device: { ...DEVICE.device, capabilities: Array.from({ length: 65 }, () => "EV_KEY") } },
    { kind: "suspend", generation: 1, requestId: "2" }, { kind: "suspend", generation: "1", requestId: "0" },
    { kind: "resume", generation: "1", requestId: "9".repeat(21) },
    { kind: "initialization-complete", generation: "1", input: INPUT },
    { kind: "suspended", generation: "1", requestId: "1" },
    { kind: "retired", generation: "0" }, { kind: "retired", generation: "1", requestId: "1" },
    { kind: "retire", generation: "1" },
  ].map(message => ({ message })))("rejects malformed/unbounded messages without reflecting their contents (%#)", ({ message }) => {
    expect(() => decodeNativeInputMessage(message)).toThrow("Invalid native input message.")
  })
})
