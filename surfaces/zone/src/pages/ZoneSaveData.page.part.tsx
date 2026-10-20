import { useRef, useSyncExternalStore } from "react"
import { fixtureModel } from "../fixtures/fixture-host"
import {
  createSaveDataStore,
  fixtureBunkerUri,
  fixtureEncryptedSecret,
  fixtureIdentity,
  fixtureManyRetiredKeys,
  fixtureOwnerKey,
  fixtureRetiredKeys,
  sceneOf,
  type SaveDataScene,
  type SaveDataStore,
} from "../fixtures/fixture-save-data"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import { ZoneKeycap } from "../ui/atoms/ZoneKeycap"
import { ZoneFoot } from "../ui/molecules/ZoneFoot"
import { ZoneTabStrip } from "../ui/molecules/ZoneTabStrip"
import { ZoneSaveData } from "./ZoneSaveData"

export const name = "Save data"
export const note = "SAVE DATA: identity on MANIA MODE's save select; back up, switch, retired keys, Korri's status"

export const composition = {
  default: [{ part: "src/ui/organisms/ZoneSaveBand.organism.part.tsx", state: "default" }],
  RetiredKeys: [{ part: "src/ui/organisms/ZoneSaveBand.organism.part.tsx", state: "MidBand" }],
  RetiredActions: [{ part: "src/ui/molecules/ZoneActionRow.molecule.part.tsx", state: "TwoButtons" }],
  ManyRetiredKeys: [{ part: "src/ui/organisms/ZoneSaveBand.organism.part.tsx", state: "Many" }],
  NotAvailable: [{ part: "src/ui/organisms/ZoneSaveBand.organism.part.tsx", state: "One" }],
  BackupReady: [{ part: "src/ui/organisms/ZoneSecretSlab.organism.part.tsx", state: "default" }],
  Password: [{ part: "src/ui/organisms/ZoneSaveEntry.organism.part.tsx", state: "default" }],
  Bunker: [{ part: "src/ui/organisms/ZoneSaveEntry.organism.part.tsx", state: "Bunker" }],
  BunkerLongUri: [{ part: "src/ui/organisms/ZoneSaveEntry.organism.part.tsx", state: "LongUri" }],
} as const

const tabItems = [
  { id: "games", label: "Games" },
  { id: "options", label: "Options", sub: "Settings + Wi-Fi" },
  { id: "savedata", label: "Save data", sub: "Identity" },
]

/** The frame's screen size: the part's frame is the device, so its window is the screen. */
function subscribeScreen(listener: () => void) {
  window.addEventListener("resize", listener)
  return () => window.removeEventListener("resize", listener)
}
const screenSize = () => `${window.innerWidth}x${window.innerHeight}`

/** Unexported: the real page around the real identity model, with a stand-in Korri behind it. */
function Scene({ initial, narrow = false, screen }: {
  initial: SaveDataScene
  narrow?: boolean
  /** The frame's size, when the part draws the page in a frame of a fixed size: at most this. */
  screen?: readonly [number, number]
}) {
  const store = useRef<SaveDataStore | null>(null)
  store.current ??= createSaveDataStore(initial)
  const scene = useSyncExternalStore(store.current.subscribe, store.current.get)
  const size = useSyncExternalStore(subscribeScreen, screenSize, () => "1280x720")
  const [windowWidth = 1280, windowHeight = 720] = size.split("x").map(Number)
  const [width, height] = screen === undefined
    ? [windowWidth, windowHeight]
    : [Math.min(windowWidth, screen[0]), Math.min(windowHeight, screen[1])]
  const layout = plan(narrow ? width * 0.36 : width, height)
  return (
    <ZoneSaveData
      card={scene.model.card}
      flow={scene.model.flow}
      frame={{
        band: layout.band,
        strips: layout.strips,
        hintBack: <ZoneKeycap button="B" word="Back" />,
        hintConfirm: <ZoneKeycap button="A" word="Confirm" />,
        tabs: <ZoneTabStrip active="savedata" focused="savedata" onSelect={() => undefined} tabs={tabItems} />,
        foot: (
          <ZoneFoot
            build={fixtureModel.buildLabel}
            clock={fixtureModel.clockLabel}
            connection={fixtureModel.network?.connection}
            device="Mini"
            strips={layout.strips}
          />
        ),
      }}
      identity={scene.identity}
      onEvent={event => store.current?.dispatch(event)}
    />
  )
}

const idle = fixtureIdentity({ _tag: "Idle" }, { retiredPublicKeys: fixtureRetiredKeys })
const draft = (text: string, layer: "letters" | "uri", focusedKey: string) => ({ text, caret: text.length, layer, focusedKey })
const backupSource = { kind: "backup", encryptedSecret: fixtureEncryptedSecret, password: "hummingbird" } as const

/** BACK UP focused, two retired keys after the SWITCH cards. */
export default function Default() {
  return <Scene initial={sceneOf(idle)} />
}

/** No backup yet: BACK UP wears NEW, and there are no retired keys. */
export function NoBackup() {
  return <Scene initial={sceneOf(fixtureIdentity())} />
}

/** A local backup exists: BACK UP wears BACKED UP. */
export function BackedUp() {
  return <Scene initial={sceneOf(fixtureIdentity({ _tag: "Idle" }, { localBackupAvailable: true, retiredPublicKeys: fixtureRetiredKeys }))} />
}

/** C16: the password for a new backup, as dots. DONE asks Korri for it. */
export function Password() {
  return <Scene initial={sceneOf(idle, { card: 0, flow: { _tag: "password", for: { kind: "export" }, draft: draft("hummingbird", "letters", "d") } })} />
}

const backupReady = sceneOf(
  fixtureIdentity({ _tag: "BackupReady", encryptedSecret: fixtureEncryptedSecret }, { localBackupAvailable: true, retiredPublicKeys: fixtureRetiredKeys }),
  { card: 0, flow: { _tag: "showSecret", onSlab: true } },
)

/** C16: Korri's backup in the band's room: the QR as the largest square, the secret on the slab, the cursor on the slab. B closes it. */
export function BackupReady() {
  return <Scene initial={backupReady} />
}

/** C16 on a 640x480 handheld: the square takes the room's height, the slab beside it. */
export function BackupReadyHandheld() {
  return (
    <ZonePartFrame shape="handheld">
      <Scene initial={backupReady} screen={[640, 480]} />
    </ZonePartFrame>
  )
}

/** C16 on a 1280x300 strip: the banner folds and the square takes the body's height. */
export function BackupReadyStrip() {
  return (
    <ZonePartFrame shape="strip-300">
      <Scene initial={backupReady} screen={[1280, 300]} />
    </ZonePartFrame>
  )
}

/** C17: the encrypted secret, half typed. */
export function SwitchSecret() {
  return <Scene initial={sceneOf(idle, { card: 1, flow: { _tag: "secret", draft: draft(fixtureEncryptedSecret.slice(0, 54), "letters", "k") } })} />
}

/** C17: the backup's password, after its secret. */
export function SwitchPassword() {
  return (
    <Scene
      initial={sceneOf(idle, {
        card: 1,
        flow: { _tag: "password", for: { kind: "switch", encryptedSecret: fixtureEncryptedSecret }, draft: draft("humming", "letters", "g") },
      })}
    />
  )
}

/** C17: what becomes of this device's local records. */
export function Disposition() {
  return <Scene initial={sceneOf(idle, { card: 1, flow: { _tag: "disposition", source: backupSource, focus: 0 } })} />
}

/** C17: the trust loss, NO under the cursor. YES switches. */
export function TrustLoss() {
  return <Scene initial={sceneOf(idle, { card: 1, flow: { _tag: "trust", source: backupSource, disposition: "transfer", focus: 0 } })} />
}

/** C18: the bunker URI on the URI layer, its prefix typed. */
export function Bunker() {
  return <Scene initial={sceneOf(idle, { card: 2, flow: { _tag: "bunker", draft: draft("bunker://", "uri", "1") } })} />
}

/** C18: a full bunker URI: the slab scrolls to keep the caret in view. */
export function BunkerLongUri() {
  return <Scene initial={sceneOf(idle, { card: 2, flow: { _tag: "bunker", draft: draft(fixtureBunkerUri, "uri", "done") } })} />
}

/** C19: a retired key focused: its first and last eight characters, and BACK UP and DELETE under the band. */
export function RetiredKeys() {
  return <Scene initial={sceneOf(idle, { card: 3, flow: { _tag: "idle" } })} />
}

/** C19: the cursor in a retired key's row, on BACK UP; A asks for its backup's password. */
export function RetiredActions() {
  return <Scene initial={sceneOf(idle, { card: 3, flow: { _tag: "retired", action: "backup" } })} />
}

/** C16 for a retired key: its backup's password; DONE sends it with the retired key. */
export function RetiredPassword() {
  const key = fixtureRetiredKeys[0] ?? ""
  return (
    <Scene
      initial={sceneOf(idle, {
        card: 3,
        flow: { _tag: "password", for: { kind: "export", retiredPublicKey: key }, draft: draft("hummingbird", "letters", "d") },
      })}
    />
  )
}

/** C19: fourteen retired keys, the cursor deep in the band. */
export function ManyRetiredKeys() {
  return (
    <Scene initial={sceneOf(fixtureIdentity({ _tag: "Idle" }, { localBackupAvailable: true, retiredPublicKeys: fixtureManyRetiredKeys }), { card: 9, flow: { _tag: "idle" } })} />
  )
}

/** C19: the first question. */
export function ConfirmDelete() {
  const key = fixtureRetiredKeys[0] ?? ""
  return <Scene initial={sceneOf(idle, { card: 3, flow: { _tag: "confirmDelete", publicKey: key, focus: 0 } })} />
}

/** C19: the second question; only its YES deletes. */
export function ConfirmBackedUp() {
  const key = fixtureRetiredKeys[0] ?? ""
  return <Scene initial={sceneOf(idle, { card: 3, flow: { _tag: "confirmBackedUp", publicKey: key, focus: 1 } })} />
}

/** C20: Korri works: its operation over the band. */
export function Working() {
  return <Scene initial={sceneOf(fixtureIdentity({ _tag: "Working", operation: "Encrypting backup" }, { retiredPublicKeys: fixtureRetiredKeys }), { card: 0, flow: { _tag: "showSecret", onSlab: false } })} />
}

/** C20: Korri's problem, verbatim, with DISMISS. */
export function Problem() {
  return (
    <Scene
      initial={sceneOf(fixtureIdentity(
        { _tag: "Problem", message: "The password does not open this backup. Check it and try again." },
        { retiredPublicKeys: fixtureRetiredKeys },
      ), { card: 1, flow: { _tag: "idle" } })}
    />
  )
}

/** C20: switched: the new owner key, shortened, with OK. */
export function Switched() {
  return <Scene initial={sceneOf(fixtureIdentity({ _tag: "Switched", ownerPublicKey: fixtureOwnerKey }, { retiredPublicKeys: fixtureRetiredKeys }), { card: 1, flow: { _tag: "idle" } })} />
}

/** No identity management on this device: one card, NOT AVAILABLE. */
export function NotAvailable() {
  return <Scene initial={sceneOf(undefined)} />
}

/** A column: the shell's narrow type, one whole card, its neighbours cut. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Scene initial={sceneOf(idle, { card: 1, flow: { _tag: "idle" } })} narrow />
    </ZonePartFrame>
  )
}
