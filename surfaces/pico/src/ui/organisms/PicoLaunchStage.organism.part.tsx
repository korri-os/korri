import { fixtureModel } from "../../fixtures/fixture-host"
import { picoScreenViewFromModel } from "../../pico-screen-view"
import { PicoLaunchStage } from "./PicoLaunchStage"

export function Running() {
  const running = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Running", kicker: "PLAYING", gameId: "hollow" },
  })
  if (running._tag !== "Running") throw new Error("Expected running fixture")
  return <PicoLaunchStage kicker={running.kicker} gameTitle={running.gameTitle} cart={running.cart} phase="running" />
}

export function UnknownGame() {
  const busy = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING" },
  })
  if (busy._tag !== "Busy") throw new Error("Expected busy fixture")
  return <PicoLaunchStage kicker={busy.kicker} detail={busy.detail} cart={busy.cart} />
}

export function NoArtwork() {
  const busy = picoScreenViewFromModel({
    ...fixtureModel, status: { _tag: "Busy", kicker: "STARTING", gameId: "petal" },
  })
  if (busy._tag !== "Busy") throw new Error("Expected busy fixture")
  return <PicoLaunchStage kicker={busy.kicker} cart={busy.cart} />
}

/** A launch the user can still stop: the cart, the slot and one row below. */
export function WithCancel() {
  return (
    <PicoLaunchStage
      actions={[{ id: "cancel", label: "Cancel", disabled: false, onPress: () => {} }]}
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="Opening your session"
      kicker="Starting Hollow…"
    />
  )
}

/** The download before a first launch: a real bar from Korri's bytes, and Cancel. */
export function Downloading10() {
  return (
    <PicoLaunchStage
      actions={[{ id: "cancel", label: "Cancel", disabled: false, onPress: () => {} }]}
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="116 MB of 1.2 GB · 6.4 MB/s · 2:43 left"
      kicker="Downloading FEX rootfs"
      progress={{ done: 116_000_000, total: 1_160_000_000 }}
    />
  )
}

export function Downloading90() {
  return (
    <PicoLaunchStage
      actions={[{ id: "cancel", label: "Cancel", disabled: false, onPress: () => {} }]}
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="1 GB of 1.2 GB · 6.4 MB/s · 0:18 left"
      kicker="Downloading FEX rootfs"
      progress={{ done: 1_044_000_000, total: 1_160_000_000 }}
    />
  )
}

/** Korri knows how far it has got but not how far it goes: a walking band. */
export function DownloadingUnknownSize() {
  return (
    <PicoLaunchStage
      actions={[{ id: "cancel", label: "Cancel", disabled: false, onPress: () => {} }]}
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="612 MB so far · 6.4 MB/s"
      kicker="Downloading FEX rootfs"
      progress={{ done: 612_000_000 }}
    />
  )
}

/** Unpacking cannot be cancelled and has no end Korri can measure: the band walks. */
export function Unpacking() {
  return (
    <PicoLaunchStage
      cart={view._tag === "Busy" ? view.cart : undefined}
      kicker="Unpacking FEX rootfs"
      progress={{ done: 0 }}
    />
  )
}

export const name = "Launch Stage"
export const note = "The cart seats into its slot while the meter fills; with Korri's bytes the same cells are the bar"

const view = picoScreenViewFromModel({
  ...fixtureModel,
  status: { _tag: "Busy", kicker: "STARTING", detail: "Waiting for the emulator", gameId: "hollow" },
})

export default function PicoLaunchStagePart() {
  return (
    <PicoLaunchStage
      cart={view._tag === "Busy" ? view.cart : undefined}
      detail="Waiting for the emulator"
      kicker="STARTING"
    />
  )
}
