import { useState } from "react"
import { fixtureModel } from "../../fixtures/fixture-host"
import { fixtureShelfGames } from "../../fixtures/named-states"
import { fixtureStarterPackFailedLong, fixtureStarterPackInstall, fixtureStarterPackRetry } from "../../fixtures/onboarding"
import type { PicoGamesView, PicoStarterPackView } from "../../pico-onboarding-view"
import { PicoGamesFound } from "./PicoGamesFound"

export const name = "Games Found"
export const note = "What Korri found to play, as carts on a floor line, a rescan where Korri offers one, and the Starter pack above it where Korri offers that"

export const composition = {
  StarterPackOffered: [{ part: "src/ui/molecules/PicoStarterPack.molecule.part.tsx", state: "default" }],
  StarterPackInstalling: [{ part: "src/ui/molecules/PicoStarterPack.molecule.part.tsx", state: "Installing" }],
  StarterPackInstallingWhileScanning: [{ part: "src/ui/molecules/PicoStarterPack.molecule.part.tsx", state: "Installing" }],
  StarterPackInstalled: [{ part: "src/ui/molecules/PicoStarterPack.molecule.part.tsx", state: "Installed" }],
  StarterPackFailedLongMessage: [{ part: "src/ui/molecules/PicoStarterPack.molecule.part.tsx", state: "FailedLongMessage" }],
} as const

const games = fixtureModel.catalog._tag === "Ready" ? fixtureShelfGames(fixtureModel.catalog.games) : []
const none: PicoStarterPackView = { _tag: "None" }
const offered: PicoStarterPackView = {
  _tag: "Offered",
  actions: [{ id: fixtureStarterPackInstall.id, label: "Install Starter pack", enabled: true }],
}

/* The Starter pack here answers as Korri would: Install starts the install,
 * and the pack's games arrive in the view below once it is in. */
function WithStarterPack({ start, library }: { readonly start: PicoStarterPackView; readonly library: PicoGamesView }) {
  const [pack, setPack] = useState(start)
  return (
    <PicoGamesFound
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onScan={() => undefined}
      onStarterPack={() => setPack({ _tag: "Installing" })}
      starterPack={pack}
      view={library}
    />
  )
}

export default function PicoGamesFoundPart() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} onStarterPack={() => undefined} starterPack={none} view={{ _tag: "Found", games, canScan: true }} />
}

export function Scanning() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} onStarterPack={() => undefined} starterPack={none} view={{ _tag: "Scanning", canContinue: false }} />
}

export function Empty() {
  return <PicoGamesFound onContinue={() => undefined} onDismissProblem={() => undefined} onScan={() => undefined} onStarterPack={() => undefined} starterPack={none} view={{ _tag: "Empty", canScan: true }} />
}

export function Failed() {
  return (
    <PicoGamesFound
      onContinue={() => undefined}
      onDismissProblem={() => undefined}
      onScan={() => undefined}
      onStarterPack={() => undefined}
      starterPack={none}
      view={{ _tag: "Failed", message: "The library could not be read.", canScan: false, problem: "The game folders could not be scanned." }}
    />
  )
}

export function StarterPackOffered() {
  return <WithStarterPack library={{ _tag: "Empty", canScan: true }} start={offered} />
}

export function StarterPackOfferedWithLibrary() {
  return <WithStarterPack library={{ _tag: "Found", games, canScan: true }} start={offered} />
}

export function StarterPackInstalling() {
  return <WithStarterPack library={{ _tag: "Empty", canScan: true }} start={{ _tag: "Installing" }} />
}

/* A rescan during the install: NEXT stays, because the install finishes by itself. */
export function StarterPackInstallingWhileScanning() {
  return <WithStarterPack library={{ _tag: "Scanning", canContinue: true }} start={{ _tag: "Installing" }} />
}

export function StarterPackInstalled() {
  return <WithStarterPack library={{ _tag: "Found", games, canScan: true }} start={{ _tag: "Installed" }} />
}

export function StarterPackFailedLongMessage() {
  return (
    <WithStarterPack
      library={{ _tag: "Empty", canScan: true }}
      start={{
        _tag: "Failed",
        message: fixtureStarterPackFailedLong,
        actions: [{ id: fixtureStarterPackRetry.id, label: "Try Starter pack again", enabled: true }],
      }}
    />
  )
}
