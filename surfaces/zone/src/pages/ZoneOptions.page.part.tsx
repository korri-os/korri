import { useRef, useSyncExternalStore } from "react"
import {
  createOptionsProgram,
  fixtureControllersGroup,
  fixtureNetworkConnecting,
  fixtureNetworkConnected,
  fixtureNetworkMany,
  fixtureNetworkNoAdapter,
  fixtureNetworkOffline,
  fixtureNetworkRejected,
  fixtureNetworkScanning,
  fixtureNetworkWired,
  fixtureOptions,
  fixturePluginsGroup,
  fixtureSeatsId,
  fixtureSettings,
  optionsCallbacks,
  type OptionsScript,
} from "../fixtures/fixture-options"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import type { ZoneOptionsSlices } from "../plan/options"
import { ZoneOptions } from "./ZoneOptions"

export const name = "Options"
export const note = "The OPTIONS tab: settings groups, SYSTEM and WI-FI rows under the banner; the keyboard and the NO / YES dialog over them"

export const composition = {
  default: [
    { part: "src/ui/templates/ZoneShell.template.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneTabStrip.molecule.part.tsx", state: "OptionsActive" },
    { part: "src/ui/molecules/ZoneFoot.molecule.part.tsx", state: "Wifi" },
    { part: "src/ui/atoms/ZoneBanner.atom.part.tsx", state: "default" },
  ],
  Choice: [{ part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "ChoiceFocused" }],
  Text: [
    { part: "src/ui/organisms/ZoneOptionsEntry.organism.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" },
  ],
  Sensitive: [
    { part: "src/ui/organisms/ZoneOptionsEntry.organism.part.tsx", state: "SecretWithClear" },
    { part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "SecretDots" },
  ],
  ActionConfirm: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "FocusedNo" }],
  Destructive: [{ part: "src/ui/molecules/ZoneDialog.molecule.part.tsx", state: "FocusedNo" }],
  Saving: [{ part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Saving" }],
  Problem: [{ part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Problem" }],
  ManyRows: [{ part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "TwelveRows" }],
  WifiSecuredJoin: [{ part: "src/ui/organisms/ZoneOptionsEntry.organism.part.tsx", state: "WifiPassword" }],
  WifiFailedPassword: [{ part: "src/ui/organisms/ZoneOptionsEntry.organism.part.tsx", state: "Rejected" }],
  Hidden: [{ part: "src/ui/organisms/ZoneOptionsEntry.organism.part.tsx", state: "HiddenName" }],
} as const

const type = (text: string): OptionsScript[] =>
  [...text].map(char => ({ _tag: "PressedKey", key: char === " " ? { kind: "space" } : { kind: "char", char } }))

/** Unexported: the real page on a live program, so every button answers. */
function Live({
  korri,
  script = [],
  share = 1,
}: {
  readonly korri: ZoneOptionsSlices
  readonly script?: readonly OptionsScript[]
  /** The share of the frame's width the page is given. */
  readonly share?: number
}) {
  const program = useRef<ReturnType<typeof createOptionsProgram> | null>(null)
  program.current ??= createOptionsProgram(korri, script)
  const live = program.current
  const snapshot = useSyncExternalStore(live.subscribe, live.get)
  // The binding measures its root; a preview reads the frame once.
  const layout = plan(innerWidth * share, innerHeight)
  return (
    <ZoneOptions
      band={layout.band}
      strips={layout.strips}
      build="korri 0.4.1 · 9a1fb22"
      clock="21:47"
      device="Mini"
      korri={snapshot.korri}
      view={snapshot.state}
      {...optionsCallbacks(live.dispatch)}
    />
  )
}

/** Device, controllers, metadata, games and system information, then SYSTEM and WI-FI; the cursor on the first row. */
export default function Default() {
  return <Live korri={fixtureOptions} script={[{ _tag: "FocusedRow", rowId: "device-name" }]} />
}

/** A choice: red triangles flank the value; left and right step it at once. */
export function Choice() {
  return <Live korri={fixtureOptions} script={[{ _tag: "FocusedRow", rowId: fixtureSeatsId }]} />
}

/** A text setting: A raised the keyboard with Korri's value as the draft. */
export function Text() {
  return <Live korri={fixtureOptions} script={[{ _tag: "PressedRow", rowId: "device-name" }]} />
}

/** A secret: dots for what is typed, and Korri's clear label as its own key. */
export function Sensitive() {
  return (
    <Live
      korri={fixtureOptions}
      script={[{ _tag: "PressedRow", rowId: "steamgriddb-credential" }, ...type("sgdb-4f2a")]}
    />
  )
}

/** An action Korri asks to confirm: its title, message and confirm label; NO is lit. */
export function ActionConfirm() {
  return <Live korri={fixtureOptions} script={[{ _tag: "PressedRow", rowId: "game-folder:roms" }]} />
}

/** A destructive device action with no words of Korri's: its label, NO and YES. */
export function Destructive() {
  return <Live korri={fixtureOptions} script={[{ _tag: "PressedRow", rowId: "zone:system:power-off" }]} />
}

/** Korri is saving one setting: its value is a blinking caret. */
export function Saving() {
  return (
    <Live
      korri={{ ...fixtureOptions, settingsStatus: { _tag: "Saving", settingId: fixtureSeatsId } }}
      script={[{ _tag: "FocusedRow", rowId: fixtureSeatsId }]}
    />
  )
}

/** Korri could not save one setting: its message in red under the row, and DISMISS under that. */
export function Problem() {
  return (
    <Live
      korri={{
        ...fixtureOptions,
        settingsStatus: {
          _tag: "Problem",
          settingId: "device-name",
          message: "That name is already used by another device on this network.",
        },
      }}
      script={[{ _tag: "FocusedRow", rowId: "zone:dismiss" }]}
    />
  )
}

/** The `system` button landed here: the cursor on SYSTEM's first row. */
export function SystemFocused() {
  return <Live korri={fixtureOptions} script={[{ _tag: "FocusSystem" }]} />
}

/** WI-FI on a network: the connection first, then SCAN, the networks and HIDDEN NETWORK. */
export function WifiConnected() {
  return <Live korri={fixtureOptions} script={[{ _tag: "FocusedRow", rowId: "zone:wifi:connection" }]} />
}

/** A scan Korri is running: SCANNING… and no networks yet. */
export function WifiScanning() {
  return (
    <Live
      korri={{ ...fixtureOptions, network: fixtureNetworkScanning }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:scan" }]}
    />
  )
}

/** Eight networks strongest first; a name with no break in it wraps. */
export function WifiList() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [], actions: [], network: fixtureNetworkMany }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:network:ap-attic" }]}
    />
  )
}

/** A secured network Korri does not know: the keyboard asks its password, in dots. */
export function WifiSecuredJoin() {
  return (
    <Live
      korri={{ ...fixtureOptions, network: fixtureNetworkConnected }}
      script={[{ _tag: "PressedRow", rowId: "zone:wifi:network:ap-attic" }, ...type("tide pool")]}
    />
  )
}

/** Korri is joining: that network's value is the caret, CONNECTING under it. */
export function WifiConnecting() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [], network: fixtureNetworkConnecting }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:network:ap-attic" }]}
    />
  )
}

/** Korri rejected the key: the message in red under the row, and the keyboard back for that network. */
export function WifiFailedPassword() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [], network: fixtureNetworkConnecting }}
      script={[{ _tag: "Publish", korri: { ...fixtureOptions, settings: [], network: fixtureNetworkRejected } }]}
    />
  )
}

/** No Wi-Fi device: the connection, then NO WI-FI ADAPTER and nothing else. */
export function WifiNoAdapter() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [fixtureControllersGroup], network: fixtureNetworkNoAdapter }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:adapter" }]}
    />
  )
}

/** On a cable: the connection reads WIRED; the networks are still listed. */
export function Wired() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [fixtureControllersGroup], network: fixtureNetworkWired }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:connection" }]}
    />
  )
}

/** No link at all: the connection reads OFFLINE. */
export function Offline() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [fixtureControllersGroup], network: fixtureNetworkOffline }}
      script={[{ _tag: "FocusedRow", rowId: "zone:wifi:connection" }]}
    />
  )
}

/** HIDDEN NETWORK: the keyboard asks the name, then the password. */
export function Hidden() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [], network: fixtureNetworkConnected }}
      script={[{ _tag: "PressedRow", rowId: "zone:wifi:hidden" }, ...type("attic-ap")]}
    />
  )
}

/** Six groups and twelve plugins, longer than any screen: the cursor deep in the list, a cut-off row at the edge. */
export function ManyRows() {
  return (
    <Live
      korri={{ ...fixtureOptions, settings: [...fixtureSettings, fixturePluginsGroup], network: fixtureNetworkMany }}
      script={[{ _tag: "FocusedRow", rowId: "plugin:mgba" }]}
    />
  )
}

/** A narrow-tall column: labels and values wrap, nothing is dropped. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Live korri={fixtureOptions} script={[{ _tag: "FocusedRow", rowId: "game-folder:roms" }]} share={0.36} />
    </ZonePartFrame>
  )
}
