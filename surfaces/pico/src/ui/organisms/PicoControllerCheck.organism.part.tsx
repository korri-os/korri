import { heardControllers, onboardingModel } from "../../fixtures/onboarding"
import { padsView } from "../../pico-onboarding-view"
import { PicoControllerCheck } from "./PicoControllerCheck"

export const name = "Controller Check"
export const note = "The controllers the host sees, lit once heard, and Korri's player seats"

export default function PicoControllerCheckPart() {
  return <PicoControllerCheck onContinue={() => undefined} onDismissProblem={() => undefined} onSeats={() => undefined} view={padsView(onboardingModel)} />
}

export function Ready() {
  return (
    <PicoControllerCheck
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onSeats={() => undefined}
      view={padsView({ ...onboardingModel, onboarding: { controllers: heardControllers } })}
    />
  )
}

export function NoControllers() {
  return (
    <PicoControllerCheck
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onSeats={() => undefined}
      view={padsView({ ...onboardingModel, onboarding: { controllers: [] } })}
    />
  )
}

export function SeatsRefused() {
  return (
    <PicoControllerCheck
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onSeats={() => undefined}
      view={padsView({
        ...onboardingModel,
        onboarding: { controllers: heardControllers },
        settingsStatus: { _tag: "Problem", settingId: "host.preferences.playerCount", message: "Stop the active session before changing playerCount" },
      })}
    />
  )
}
