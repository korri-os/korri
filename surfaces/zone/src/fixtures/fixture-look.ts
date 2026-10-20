/**
 * The look a Caliper frame shows (fixture code only; the product's look is
 * the root's `data-look`, from OPTIONS' LOOK row).
 *
 * `ZONE_LOOK=modern bun run caliper` (or blend, retro; blend without it)
 * makes vite.config.ts name the look on every frame's `<html>` before any
 * part runs, so every part, page or atom, is drawn in it: the look's token
 * blocks in zone-tokens.css match `[data-look] .zone-theme`, and the hooks
 * that draw in TypeScript read `--zone-look`. A part that shows the LOOK
 * row reads the frame's look here, and stepping the row switches the frame
 * at once, as the product does.
 */
import { isZoneLook, ZONE_DEFAULT_LOOK, type ZoneLook } from "../plan/look"

export function fixtureFrameLook(): ZoneLook {
  const named = typeof document === "undefined" ? undefined : document.documentElement.dataset.look
  return isZoneLook(named) ? named : ZONE_DEFAULT_LOOK
}

export function fixtureShowLook(look: ZoneLook): void {
  if (typeof document !== "undefined") document.documentElement.dataset.look = look
}
