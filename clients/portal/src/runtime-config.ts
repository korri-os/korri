export interface LinuxRuntimeConfig {
  readonly korridPort: number
  readonly korridCapability: string
  readonly surfaceId?: string
}

export async function loadLinuxRuntimeConfig(
  fetcher: (input: string, init: RequestInit) => Promise<Response> = globalThis.fetch,
): Promise<LinuxRuntimeConfig> {
  const response = await fetcher("/runtime.json", {
    cache: "no-store",
    credentials: "same-origin",
  })
  if (!response.ok) {
    throw new Error(`Linux runtime configuration returned ${response.status}`)
  }

  const value: unknown = await response.json()
  if (!isLinuxRuntimeConfig(value)) {
    throw new Error("invalid Linux runtime configuration")
  }
  return value
}

function isLinuxRuntimeConfig(value: unknown): value is LinuxRuntimeConfig {
  if (typeof value !== "object" || value === null) return false
  const candidate = value as Record<string, unknown>
  return Number.isInteger(candidate.korridPort)
    && Number(candidate.korridPort) >= 1
    && Number(candidate.korridPort) <= 65_535
    && typeof candidate.korridCapability === "string"
    && /^[0-9a-f]{64}$/.test(candidate.korridCapability)
    && (candidate.surfaceId === undefined
      || (typeof candidate.surfaceId === "string" && candidate.surfaceId !== ""))
}
