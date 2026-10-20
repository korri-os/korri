import { useState } from "react"
import {
  fixtureStarterPackFailedLong,
  fixtureStarterPackInstall,
  fixtureStarterPackRetry,
} from "../../fixtures/onboarding"
import type { PicoStarterPackView } from "../../pico-onboarding-view"
import { PicoStarterPack } from "./PicoStarterPack"

export const name = "Starter Pack"
export const note = "The Starter pack offered during setup: Install, the install under way, done, and Korri's failure with a retry"

/* Install and the retry start the install here, as Korri would answer them. */
function Stateful({ start }: { readonly start: PicoStarterPackView }) {
  const [view, setView] = useState(start)
  return <PicoStarterPack onAction={() => setView({ _tag: "Installing" })} view={view} />
}

export default function Offered() {
  return <Stateful start={{ _tag: "Offered", actions: [{ id: fixtureStarterPackInstall.id, label: "Install Starter pack", enabled: true }] }} />
}

/* Korri dims an action it cannot run now; the treaty asks for it dimmed, not hidden. */
export function OfferedDimmed() {
  return <Stateful start={{ _tag: "Offered", actions: [{ id: fixtureStarterPackInstall.id, label: "Install Starter pack", enabled: false }] }} />
}

export function Installing() {
  return <Stateful start={{ _tag: "Installing" }} />
}

export function Installed() {
  return <Stateful start={{ _tag: "Installed" }} />
}

export function Failed() {
  return (
    <Stateful start={{
      _tag: "Failed",
      message: "The Starter pack could not be installed. Try again.",
      actions: [{ id: fixtureStarterPackRetry.id, label: "Try Starter pack again", enabled: true }],
    }} />
  )
}

export function FailedLongMessage() {
  return (
    <Stateful start={{
      _tag: "Failed",
      message: fixtureStarterPackFailedLong,
      actions: [{ id: fixtureStarterPackRetry.id, label: "Try Starter pack again", enabled: true }],
    }} />
  )
}
