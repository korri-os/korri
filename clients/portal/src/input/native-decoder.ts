import type { NativeInputControl, NativeInputEvent } from "@contracts/generated/korrid"

export type NativeInputMessage = NativeInputEvent | NativeInputControl
const CLASSES = new Set(["gamepad", "keyboard", "mouse", "touch", "system", "unknown"])
const record = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value)
const finite = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value)
const integer = (value: unknown, min: number, max: number) => finite(value) && Number.isInteger(value) && value >= min && value <= max
const id = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 1024
const identity = (value: unknown): value is string => typeof value === "string" && /^[1-9][0-9]{0,19}$/.test(value)
const keys = (value: Record<string, unknown>, allowed: readonly string[]) => Object.keys(value).every(key => allowed.includes(key))
const deviceClass = (value: unknown) => typeof value === "string" && CLASSES.has(value)

/** Runtime validation of the Rust-owned treaty, not a second event language. */
export function decodeNativeInputMessage(value: unknown): NativeInputMessage {
  if (!record(value)) throw new Error("Invalid native input message.")
  let valid = false
  switch (value.kind) {
    case "initialization-complete":
    case "retired":
      valid = keys(value, ["kind", "generation"]) && identity(value.generation)
      break
    case "device-state-complete":
      valid = keys(value, ["kind", "deviceId"]) && id(value.deviceId)
      break
    case "suspend":
    case "resume":
      valid = keys(value, ["kind", "generation", "requestId"]) && identity(value.generation) && identity(value.requestId)
      break
    case "device-added": {
      const device = value.device
      valid = keys(value, ["kind", "device"]) && record(device)
        && keys(device, ["deviceId", "class", "name", "capabilities", "axes"])
        && id(device.deviceId) && deviceClass(device.class) && typeof device.name === "string"
        && Array.isArray(device.capabilities) && device.capabilities.length <= 64
        && device.capabilities.every(capability => typeof capability === "string" && capability.length <= 128)
        && (device.axes === undefined || (Array.isArray(device.axes) && device.axes.length <= 64
          && device.axes.every(axis => record(axis) && keys(axis, ["code", "minimum", "maximum", "flat"])
            && integer(axis.code, 0, 63) && finite(axis.minimum) && finite(axis.maximum)
            && (axis.flat === undefined || finite(axis.flat)))
          && new Set(device.axes.map(axis => axis.code)).size === device.axes.length))
      break
    }
    case "device-removed":
      valid = keys(value, ["kind", "deviceId"]) && id(value.deviceId)
      break
    case "input":
      valid = keys(value, ["kind", "deviceId", "class", "type", "code", "value", "timestamp"])
        && id(value.deviceId) && deviceClass(value.class) && integer(value.type, 0, 31)
        && integer(value.code, 0, value.type === 3 ? 63 : 767)
        && integer(value.value, value.type === 1 ? 0 : -2147483648, value.type === 1 ? 2 : 2147483647)
        && finite(value.timestamp)
      break
    case "action":
      valid = keys(value, ["kind", "class", "action", "timestamp"])
        && deviceClass(value.class) && value.action === "system" && finite(value.timestamp)
      break
  }
  if (!valid) throw new Error("Invalid native input message.")
  return value as unknown as NativeInputMessage
}
