import { isZoneLook, ZONE_DEFAULT_LOOK, type ZoneLook } from "./plan/look"

/**
 * Which look this device shows Zone in, kept by Zone itself.
 *
 * The storage seam, stated plainly: one key, `zone.look`, holding one look
 * id (`modern`, `blend` or `retro`) and nothing else. It is grounded in
 * Pico's own seam for the same kind of choice, `pico.font`
 * (surfaces/pico/src/pico-font-preference.ts), and mirrors it exactly. A look
 * is a display preference (no identity, no capability, no secret), so it
 * lives in the browser's local storage, which the device keeps across
 * restarts, the same way the portal keeps its `korri.surface`. Korri is
 * never told: a look is Zone's business, and the treaty has no word for it.
 *
 * A missing, unreadable, blocked or unknown value means the default look,
 * Blend. A tampered value can do no more than pick another of Zone's own
 * looks.
 */
export const ZONE_LOOK_KEY = "zone.look"

function storage(): Storage | undefined {
  try {
    return typeof window === "undefined" ? undefined : window.localStorage
  } catch {
    /* Reading `localStorage` itself throws where storage is blocked. */
    return undefined
  }
}

export function readZoneLook(): ZoneLook {
  try {
    const stored = storage()?.getItem(ZONE_LOOK_KEY)
    return isZoneLook(stored) ? stored : ZONE_DEFAULT_LOOK
  } catch {
    /* A look is not worth failing a screen over: unreadable means default. */
    return ZONE_DEFAULT_LOOK
  }
}

export function rememberZoneLook(look: ZoneLook): void {
  try {
    storage()?.setItem(ZONE_LOOK_KEY, look)
  } catch {
    /* Full or disabled storage: the look still applies until Zone reloads. */
  }
}
