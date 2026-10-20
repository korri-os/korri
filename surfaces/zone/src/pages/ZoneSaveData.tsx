import "./ZoneSaveData.css"
import type {
  SurfaceIdentityDisposition,
  SurfaceIdentityManagement,
} from "@contracts/surface/korri-surface"
import { useEffect, useRef, type FocusEvent, type ReactNode } from "react"
import { shortKey } from "../plan/save-data"
import { ZoneBanner } from "../ui/atoms/ZoneBanner"
import { ZoneQrWell } from "../ui/atoms/ZoneQrWell"
import { ZoneBusyStrip } from "../ui/molecules/ZoneBusyStrip"
import { ZoneDialog, type ZoneDialogAction } from "../ui/molecules/ZoneDialog"
import type { ZoneKey, ZoneKeyboardLayer } from "../ui/molecules/ZoneKeyboard"
import { ZoneSaveBand, type ZoneSaveBandCard } from "../ui/organisms/ZoneSaveBand"
import { ZoneSaveEntry } from "../ui/organisms/ZoneSaveEntry"
import { ZoneSecretSlab } from "../ui/organisms/ZoneSecretSlab"
import { ZoneShell, type ZoneBand, type ZoneStrips } from "../ui/templates/ZoneShell"

/** What is typed so far in a keyboard step. */
export interface ZoneSaveDataDraft {
  readonly text: string
  readonly caret: number
  readonly layer: ZoneKeyboardLayer
  readonly focusedKey: string
}

/**
 * The step of the identity flow the page draws, as the page reads it. The
 * identity model's own flow (src/state/identity.ts) carries more and is
 * passed here unchanged; test/zone-save-data.test.tsx holds the two
 * assignable, so the binding needs no mapping.
 */
export type ZoneSaveDataFlow =
  | { readonly _tag: "idle" }
  | { readonly _tag: "showSecret"; readonly onSlab: boolean }
  | { readonly _tag: "password" | "secret" | "bunker"; readonly draft: ZoneSaveDataDraft }
  | { readonly _tag: "disposition" | "trust"; readonly focus: number }
  | { readonly _tag: "confirmDelete" | "confirmBackedUp"; readonly publicKey: string; readonly focus: number }

/** What the page reports. Each is one of the identity model's messages. */
export type ZoneSaveDataEvent =
  | { readonly _tag: "FocusedSaveCard"; readonly index: number }
  | { readonly _tag: "FocusedSecret" }
  | { readonly _tag: "ChoseSaveCard"; readonly index: number }
  | { readonly _tag: "FocusedKey"; readonly key: string }
  | { readonly _tag: "TypedChar"; readonly char: string }
  | { readonly _tag: "TypedSpace" }
  | { readonly _tag: "DeletedChar" }
  | { readonly _tag: "SwitchedLayer"; readonly layer: ZoneKeyboardLayer }
  | { readonly _tag: "PressedDone" }
  | { readonly _tag: "FocusedChoice"; readonly index: 0 | 1 }
  | { readonly _tag: "ChoseDisposition"; readonly disposition: SurfaceIdentityDisposition }
  | { readonly _tag: "Answered"; readonly yes: boolean }
  | { readonly _tag: "DismissedStatus" }

/** The shell's slots, filled by the binding the same way for every page. */
export interface ZoneSaveDataFrame {
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
  readonly hintBack?: ReactNode
  readonly hintConfirm?: ReactNode
  readonly tabs?: ReactNode
  readonly foot?: ReactNode
  readonly footEnd?: ReactNode
}

const keyEvent = (key: ZoneKey): ZoneSaveDataEvent => {
  switch (key.kind) {
    case "char":
      return { _tag: "TypedChar", char: key.char }
    case "space":
      return { _tag: "TypedSpace" }
    case "delete":
      return { _tag: "DeletedChar" }
    case "done":
      return { _tag: "PressedDone" }
    case "layer":
      return { _tag: "SwitchedLayer", layer: key.layer }
  }
}

const CAPTIONS = { password: "Password", secret: "Backup", bunker: "Bunker" } as const

/** The cards, in band order. The words are the page's labels; the keys are Korri's. */
function cardsOf(identity: SurfaceIdentityManagement | undefined, marked: boolean): readonly ZoneSaveBandCard[] {
  if (identity === undefined) {
    return [{ id: "unavailable", header: "Identity", body: { kind: "text", text: "Not available" } }]
  }
  const { status } = identity
  const backup: ZoneSaveBandCard = {
    id: "backup",
    header: "Back up",
    body: status._tag === "BackupReady"
      ? {
        kind: "node",
        node: (
          <span className="zone-save-data-qr" data-clear={marked ? "triangles" : undefined}>
            <ZoneQrWell label="Backup QR code" value={status.encryptedSecret} />
          </span>
        ),
      }
      : { kind: "text", text: "This key" },
    tag: identity.localBackupAvailable ? { kind: "saved", label: "Backed up" } : { kind: "open", label: "New" },
  }
  return [
    backup,
    { id: "switch-backup", header: "Switch", body: { kind: "text", text: "From backup" } },
    { id: "switch-bunker", header: "Switch", body: { kind: "text", text: "Bunker" } },
    ...identity.retiredPublicKeys.map((key): ZoneSaveBandCard => ({
      id: `retired:${key}`,
      header: "Retired",
      body: { kind: "text", text: shortKey(key) },
      tag: { kind: "remote", label: "Delete" },
    })),
  ]
}

/**
 * The SAVE DATA page: identity on MANIA MODE's save select.
 *
 * The shell with SAVE DATA as its tab; in the body the SAVE DATA banner and
 * a band of vertical save cards: BACK UP (tagged NEW, or BACKED UP once a
 * local backup exists), SWITCH from a backup, SWITCH to a bunker, then one
 * RETIRED card per retired key with a DELETE tag. Over the band, the step
 * of the flow the player is in: a keyboard with a caption, or a blue
 * dialog.
 *
 * Korri's status is drawn as Korri states it. Working is the busy strip
 * over the band with the operation as its kicker. BackupReady turns the
 * BACK UP card's well into the QR code and lays the secret on a white slab
 * beside or under the band; the cursor goes to the slab, and the band goes
 * unmarked so the QR code can take the whole well. Problem and Switched are
 * dialogs.
 *
 * A shape: it takes Korri's identity slice and the identity model's value
 * as props and reports every press and every focus through `onEvent`.
 * Focus is the host's: the page moves DOM focus onto the cursor when a step
 * opens or closes, and hears where the host moves it after that.
 */
export function ZoneSaveData({
  identity,
  card,
  flow,
  frame = {},
  onEvent,
}: {
  /** Korri's identity management; absent when this device cannot manage identity. */
  readonly identity: SurfaceIdentityManagement | undefined
  readonly card: number
  readonly flow: ZoneSaveDataFlow
  readonly frame?: ZoneSaveDataFrame
  readonly onEvent?: (event: ZoneSaveDataEvent) => void
}) {
  const root = useRef<HTMLDivElement>(null)
  const status = identity?.status
  const send = (event: ZoneSaveDataEvent) => onEvent?.(event)
  const backup = status?._tag === "BackupReady" ? status.encryptedSecret : undefined
  // The cursor on the backup's slab leaves the band unmarked, so the QR code
  // in BACK UP's well needs no room for the card's triangles.
  const marked = !(backup !== undefined && flow._tag === "showSecret" && flow.onSlab)
  const cards = cardsOf(identity, marked)

  // When a step opens or closes, the cursor goes to it: DOM mechanics only.
  // The first render leaves focus where the host has it.
  const scene = `${status?._tag ?? "none"}:${flow._tag}`
  const shown = useRef(scene)
  useEffect(() => {
    if (shown.current === scene) return
    shown.current = scene
    const page = root.current
    if (page === null) return
    const target = page.querySelector<HTMLElement>(".zone-save-data-layer [data-focused]")
      ?? page.querySelector<HTMLElement>(".zone-save-data-layer button")
      ?? page.querySelector<HTMLElement>(".zone-save-data-slab [tabindex]")
      ?? page.querySelector<HTMLElement>(".zone-save-data-band button[data-focused]")
    target?.focus()
  }, [scene])

  // The host moved the cursor across a dialog's buttons.
  const heardChoice = (event: FocusEvent<HTMLDivElement>) => {
    const buttons = [...event.currentTarget.querySelectorAll("[role='dialog'] button")]
    const index = buttons.indexOf(event.target)
    if (buttons.length === 2 && (index === 0 || index === 1)) send({ _tag: "FocusedChoice", index })
  }

  const dialog = (headline: string, lines: readonly string[], actions: readonly ZoneDialogAction[], focus: number, title?: string) => (
    <div className="zone-save-data-layer" data-layer="dialog" onFocus={heardChoice}>
      <ZoneDialog actions={actions} focused={focus} headline={headline} lines={lines} title={title} />
    </div>
  )

  const answer = (yes: boolean) => () => send({ _tag: "Answered", yes })
  const noYes: readonly ZoneDialogAction[] = [
    { label: "No", onPress: answer(false) },
    { label: "Yes", onPress: answer(true) },
  ]

  let layer: ReactNode = null
  if (status?._tag === "Problem") {
    layer = dialog("Identity", [status.message], [{ label: "Dismiss", onPress: () => send({ _tag: "DismissedStatus" }) }], 0)
  } else if (status?._tag === "Switched") {
    layer = dialog("Switched", [shortKey(status.ownerPublicKey)], [{ label: "OK", onPress: () => send({ _tag: "DismissedStatus" }) }], 0)
  } else {
    switch (flow._tag) {
      case "idle":
      case "showSecret":
        break
      case "password":
      case "secret":
      case "bunker":
        layer = (
          <div className="zone-save-data-layer" data-layer="keyboard">
            <ZoneSaveEntry
              caption={CAPTIONS[flow._tag]}
              caret={flow.draft.caret}
              draft={flow.draft.text}
              focusedKey={flow.draft.focusedKey}
              layer={flow.draft.layer}
              onFocusKey={key => send({ _tag: "FocusedKey", key })}
              onKey={key => send(keyEvent(key))}
              secret={flow._tag === "password"}
            />
          </div>
        )
        break
      case "disposition":
        layer = dialog("Local records", ["Transfer them to the new key, or delete them"], [
          { label: "Transfer", onPress: () => send({ _tag: "ChoseDisposition", disposition: "transfer" }) },
          { label: "Delete", onPress: () => send({ _tag: "ChoseDisposition", disposition: "delete" }) },
        ], flow.focus)
        break
      case "trust":
        layer = dialog("Switch key?", ["Other devices will stop trusting this key"], noYes, flow.focus)
        break
      case "confirmDelete":
        layer = dialog("Delete this key?", ["Back it up first."], noYes, flow.focus, shortKey(flow.publicKey))
        break
      case "confirmBackedUp":
        layer = dialog("Is it backed up?", [], noYes, flow.focus, shortKey(flow.publicKey))
        break
    }
  }

  return (
    <ZoneShell {...frame}>
      <div className="zone-save-data" ref={root}>
        <div className="zone-save-data-banner">
          <ZoneBanner>Save data</ZoneBanner>
        </div>
        {status?._tag === "Working" ? (
          <div className="zone-save-data-busy">
            <ZoneBusyStrip kicker={status.operation} />
          </div>
        ) : null}
        <div className="zone-save-data-main" data-slab={backup === undefined ? undefined : "shown"}>
          <div className="zone-save-data-band">
            <ZoneSaveBand
              cards={cards}
              focusedIndex={card}
              marked={marked}
              onChoose={index => send({ _tag: "ChoseSaveCard", index })}
              onFocus={index => send({ _tag: "FocusedSaveCard", index })}
            />
          </div>
          {backup === undefined ? null : (
            <div className="zone-save-data-slab">
              <ZoneSecretSlab focused={!marked} label="Backup secret" onFocus={() => send({ _tag: "FocusedSecret" })} secret={backup} />
            </div>
          )}
        </div>
        {layer}
      </div>
    </ZoneShell>
  )
}
