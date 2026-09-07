import type { KorriNativeBridgeSurface } from "@contracts/bridge/korri-native-bridge"
import type { KorriRpcBridgeSurface } from "@contracts/bridge/korri-rpc-bridge"
import {
  createInMemoryLauncherBridge,
  createKorriNativeLauncherBridge,
  type LauncherBridge,
} from "../bridge/launcher-bridge"
import { createUnavailableLauncherBridge } from "../bridge/unavailable-launcher-bridge"
import {
  createHttpKorridClient,
  createInMemoryKorridClient,
  type KorridClient,
} from "./client"

export interface PortalConnection {
  readonly korrid: KorridClient
  readonly bridge: LauncherBridge
}

/** The same credential binding selects real RPC on every device. */
export function createPortalConnection(
  rpc: KorriRpcBridgeSurface | undefined,
  native?: KorriNativeBridgeSurface,
): PortalConnection {
  if (rpc === undefined && native === undefined) {
    return {
      korrid: createInMemoryKorridClient(),
      bridge: createInMemoryLauncherBridge(),
    }
  }

  let port: number
  let capability: string
  try {
    if (rpc === undefined) throw new Error()
    port = rpc.korridPort()
    capability = rpc.korridCapability()
    if (
      !Number.isInteger(port) || port < 1 || port > 65535 ||
      typeof capability !== "string" || capability.length === 0 ||
      capability.trim() !== capability || /[\r\n]/.test(capability)
    ) throw new Error()
  } catch {
    // A broken native binding is not browser development. Do not hide its
    // failure with sample data or print a credential-bearing native exception.
    throw new Error("The shell did not provide a valid korrid connection.")
  }

  return {
    korrid: createHttpKorridClient(`http://127.0.0.1:${port}`, capability),
    bridge: native === undefined
      ? createUnavailableLauncherBridge()
      : createKorriNativeLauncherBridge(native),
  }
}
