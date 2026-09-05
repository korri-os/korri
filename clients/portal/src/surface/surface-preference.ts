import { DEFAULT_SURFACE_ID, portalSurfaceById } from "./surface-registry"

/** Query parameter that selects a surface, e.g. `?surface=pico`. */
export const SURFACE_PARAM = "surface"

const STORAGE_KEY = "korri.surface"

/**
 * Which surface the user wants.
 *
 * The storage seam, stated plainly: this is a display preference and nothing
 * else — no identity, no capability, no secret — so it lives in local storage
 * where the device keeps it across restarts of a WebView that has no other
 * memory. A tampered value cannot do more than pick a different theme, and an
 * unrecognised one is ignored rather than trusted.
 *
 * The query parameter wins and persists, so setting a surface once on a device
 * that can only load a fixed URL survives the next boot.
 */
export function resolveSurfacePreference(location: {
  readonly search: string
}, storage?: Storage): string {
  const requested = new URLSearchParams(location.search).get(SURFACE_PARAM)
  if (requested !== null && portalSurfaceById(requested) !== undefined) {
    remember(requested, storage)
    return requested
  }

  const remembered = read(storage)
  if (remembered !== undefined && portalSurfaceById(remembered) !== undefined) {
    return remembered
  }
  return DEFAULT_SURFACE_ID
}

function read(storage?: Storage): string | undefined {
  try {
    return storage?.getItem(STORAGE_KEY) ?? undefined
  } catch {
    /* Storage can be disabled or full. A theme preference is not worth failing
     * a boot over, so an unreadable store means "no preference". */
    return undefined
  }
}

function remember(id: string, storage?: Storage): void {
  try {
    storage?.setItem(STORAGE_KEY, id)
  } catch {
    /* Same reasoning: the choice still applies to this session. */
  }
}
