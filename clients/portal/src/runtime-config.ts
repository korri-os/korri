import type { KorriRpcBridgeSurface } from "@contracts/bridge/korri-rpc-bridge"

export interface LinuxRuntimeConfig {
  readonly korridPort: number
  readonly korridCapability: string
}

export function readLinuxRuntimeConfig(
  binding: KorriRpcBridgeSurface | undefined,
): LinuxRuntimeConfig {
  try {
    if (binding === undefined) throw new Error()
    const korridPort = binding.korridPort()
    const korridCapability = binding.korridCapability()
    if (
      !Number.isInteger(korridPort)
      || korridPort < 1
      || korridPort > 65_535
      || typeof korridCapability !== "string"
      || !/^[0-9a-f]{64}$/.test(korridCapability)
    ) throw new Error()
    return { korridPort, korridCapability }
  } catch {
    throw new Error("Linux did not provide a valid korrid connection.")
  }
}
