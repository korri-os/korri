/**
 * First-boot setup's program: OnboardingNav, run by usePicoProgram, plus its
 * subscriptions: the host's Back, and every model Korri publishes, so
 * `updateOnboarding` can notice a finished scan, connect, save or switch.
 * Setup listens to no other host button: System, Options and Menu open
 * screens that belong to the console after setup.
 */
import type { SurfaceHost, SurfaceModel } from "@contracts/surface/korri-surface"
import { useEffect } from "react"
import { performPicoRequest, subscribePicoHostButtons } from "./pico-host"
import { initialOnboarding, type OnboardingMessage, type OnboardingNav } from "./state/onboarding"
import { updateOnboarding } from "./state/onboarding-update"
import { usePicoProgram } from "./use-pico-program"

export function usePicoOnboarding(
  korri: SurfaceModel,
  host: SurfaceHost,
): { readonly nav: OnboardingNav; readonly dispatch: (message: OnboardingMessage) => void } {
  const { model: nav, dispatch } = usePicoProgram<OnboardingNav, OnboardingMessage, SurfaceModel>(
    initialOnboarding,
    updateOnboarding,
    korri,
    request => performPicoRequest(host, { chooseFont: () => undefined }, request),
  )
  useEffect(() => subscribePicoHostButtons(host, button => {
    if (button._tag === "PressedBack") dispatch(button)
  }, ["PressedBack"]), [host, dispatch])
  useEffect(() => {
    dispatch({ _tag: "KorriPublished" })
  }, [korri, dispatch])
  return { nav, dispatch }
}
