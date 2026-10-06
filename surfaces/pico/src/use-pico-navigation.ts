/**
 * The catalog's program: PicoNavigation, run by usePicoProgram, plus its
 * subscriptions.
 *
 * - The host's four buttons, registered once per host.
 * - Korri's model: every model Korri publishes is a message, so `update` can
 *   notice that a played game ended, a save finished or a backup is ready.
 *   Korri replaces its model whole, so a new object means news.
 * - The idle timer, running only while attract is allowed and restarted by
 *   every press and focus move.
 */
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useCallback, useEffect } from "react"
import { PICO_ATTRACT_AFTER_MS } from "./pico-attract"
import type { PicoFontId } from "./pico-fonts"
import { performPicoRequest, subscribePicoHostButtons } from "./pico-host"
import type { PicoInitialView } from "./pico-initial-view"
import type { PicoMessage } from "./state/messages"
import { initialNavigation, type PicoNavigation } from "./state/navigation"
import type { PicoRequest } from "./state/requests"
import { attractShowing, canAttract } from "./state/shown"
import { update } from "./state/update"
import { usePicoProgram } from "./use-pico-program"

export interface PicoNavigationProgram {
  readonly nav: PicoNavigation
  readonly dispatch: (message: PicoMessage) => void
  /** Whether attract is on screen this instant. A capture handler reads it to
   * swallow the press that wakes the screen. */
  readonly attractShowingNow: () => boolean
}

export function usePicoNavigation(
  korri: SurfaceModel,
  host: SurfaceHost,
  chooseFont: (font: PicoFontId) => void,
  initial?: PicoInitialView,
): PicoNavigationProgram {
  const perform = (request: PicoRequest) => performPicoRequest(host, { chooseFont }, request)
  const { model: nav, dispatch, latest } = usePicoProgram<PicoNavigation, PicoMessage, SurfaceModel>(
    () => initialNavigation(initial), update, korri, perform)

  useEffect(() => subscribePicoHostButtons(host, dispatch), [host, dispatch])

  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [korri, dispatch])

  const allowed = nav.idle._tag === "Awake" && canAttract(nav, korri)
  useEffect(() => {
    if (!allowed) return
    const timer = setTimeout(() => dispatch({ _tag: "IdleElapsed" }), PICO_ATTRACT_AFTER_MS)
    return () => clearTimeout(timer)
  }, [allowed, nav.idle.activity, dispatch])

  const attractShowingNow = useCallback(() => {
    const now = latest()
    return attractShowing(now.model, now.context)
  }, [latest])

  return { nav, dispatch, attractShowingNow }
}
