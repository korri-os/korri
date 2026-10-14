import "./PicoOnboarding.css"
import { useEffect, useRef } from "react"
import type { PicoOnboardingScreen } from "../pico-onboarding-view"
import { IDENTITY_FORM } from "../state/navigation"
import { PICO_IDENTITY_BACKUP_ACTION } from "../pico-settings-view"
import type { IdentityOrigin } from "../state/onboarding"
import { type PicoStep, PicoStepTrack } from "../ui/atoms/PicoStepTrack"
import type { PicoButtonBarHint } from "../ui/molecules/PicoButtonBar"
import type { PicoTyping } from "../ui/molecules/PicoKeyboard"
import { PicoControllerCheck } from "../ui/organisms/PicoControllerCheck"
import { PicoGamesFound } from "../ui/organisms/PicoGamesFound"
import { PicoIdentityDialog } from "../ui/organisms/PicoIdentityDialog"
import { PicoIdentitySetup } from "../ui/organisms/PicoIdentitySetup"
import { PicoSetupDone } from "../ui/organisms/PicoSetupDone"
import { PicoSetupWelcome } from "../ui/organisms/PicoSetupWelcome"
import { PicoTextEditor } from "../ui/organisms/PicoTextEditor"
import { type PicoWifiActions, PicoWifiSetup } from "../ui/organisms/PicoWifiSetup"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

/** Everything a person can do during setup. The owner turns each into a message. */
export interface PicoOnboardingActions {
  readonly onStart: () => void
  readonly onContinue: () => void
  readonly wifi: PicoWifiActions
  readonly typing: PicoTyping
  readonly onToggleShown: () => void
  readonly onSubmitText: () => void
  /** OK on a refusal Korri reported for a setting. */
  readonly onDismissProblem: () => void
  readonly identity: {
    readonly onChoose: (how: IdentityOrigin) => void
    readonly onEdit: (field: "backup" | "password" | "uri") => void
    readonly onSubmit: () => void
    readonly onBackUp: () => void
  }
  readonly backup: {
    readonly onEdit: (password: string) => void
    readonly onSubmit: () => void
    readonly onClose: () => void
  }
  /** A on the player-seats row: Korri's next value for it. */
  readonly onSeats: () => void
  readonly onScanGames: () => void
  readonly onFinish: () => void
}

const SELECT_BACK: readonly PicoButtonBarHint[] = [
  { hintKey: "a", label: "SELECT" },
  { hintKey: "b", label: "BACK" },
]
const BACK_ONLY: readonly PicoButtonBarHint[] = [{ hintKey: "b", label: "BACK" }]
const NONE: readonly PicoButtonBarHint[] = []

/** What A and B do on this screen. B on Hello does nothing, so it is not named;
 * nor while Korri switches an identity, which cannot be stopped. */
function hintsFor(screen: PicoOnboardingScreen): readonly PicoButtonBarHint[] {
  switch (screen._tag) {
    case "Hello":
      return [{ hintKey: "a", label: "START" }]
    case "Wifi":
      return screen.view._tag === "Scanning" || screen.view._tag === "Connecting" ? BACK_ONLY : SELECT_BACK
    case "Identity":
      return screen.view._tag === "Working" || screen.view._tag === "Approving" ? NONE : SELECT_BACK
    case "Games":
      return screen.view._tag === "Scanning" ? BACK_ONLY : SELECT_BACK
    case "Play":
      return screen.finishing ? NONE : SELECT_BACK
    case "Entry":
    case "Pads":
      return SELECT_BACK
  }
}

/**
 * First-boot setup: one screen per step, in Pico's own shell. The status bar
 * names the step (SETUP WI-FI), or what the keyboard is typing; a track under
 * it shows the sequence; the floor names what A and B do. The step's content
 * fills the rest.
 *
 * Controlled: the screen to draw comes in, every press goes out. A new
 * screen puts the cursor on its first control, or its last where that is the
 * safe answer (`cursor`), because the host's next confirm must press
 * something the player can see.
 */
export function PicoOnboarding({
  screen,
  track,
  place,
  cursor,
  actions,
}: {
  readonly screen: PicoOnboardingScreen
  readonly track: readonly PicoStep[]
  /** What the status bar names. */
  readonly place: string
  /** The screen's identity and where its cursor starts. */
  readonly cursor: { readonly key: string; readonly at: "first" | "last" }
  readonly actions: PicoOnboardingActions
}) {
  const body = useRef<HTMLDivElement>(null)
  const backingUp = screen._tag === "Identity" && screen.backup !== undefined
  useEffect(() => {
    if (backingUp) return
    const buttons = body.current?.querySelectorAll<HTMLElement>("button:not([disabled])")
    if (buttons === undefined || buttons.length === 0) return
    ;(cursor.at === "last" ? buttons[buttons.length - 1] : buttons[0])?.focus()
  }, [cursor.key, cursor.at, backingUp])

  return (
    <PicoScreenShell hints={hintsFor(screen)} label={place} place="SETUP">
      <div className="pico-onboarding">
        <PicoStepTrack steps={track} />
        <div className="pico-onboarding-step" ref={body}>
          {screen._tag === "Hello" ? <PicoSetupWelcome askWifi={screen.askWifi} onStart={actions.onStart} /> : null}
          {screen._tag === "Wifi" ? <PicoWifiSetup actions={actions.wifi} view={screen.view} /> : null}
          {screen._tag === "Entry" ? (
            <PicoTextEditor
              actions={[{ label: screen.entry.submitLabel, disabled: !screen.entry.ready, onPress: actions.onSubmitText }]}
              busy={screen.entry.busy}
              draft={screen.entry.draft}
              formLabel={screen.entry.fieldLabel}
              key={cursor.key}
              label={screen.entry.fieldLabel}
              maxLength={screen.entry.maxLength}
              note={screen.entry.note}
              placeholder={screen.entry.placeholder}
              problem={screen.entry.problem === undefined
                ? undefined
                : { message: screen.entry.problem, onDismiss: actions.onDismissProblem }}
              prompt={screen.entry.prompt}
              reveal={screen.entry.secret ? { shown: screen.entry.shown, onToggle: actions.onToggleShown } : undefined}
              secret={screen.entry.secret}
              typing={actions.typing}
            />
          ) : null}
          {screen._tag === "Identity" ? (
            <PicoIdentitySetup
              actions={{ ...actions.identity, onContinue: actions.onContinue }}
              view={screen.view}
            />
          ) : null}
          {screen._tag === "Pads" ? (
            <PicoControllerCheck
              onContinue={actions.onContinue}
              onDismissProblem={actions.onDismissProblem}
              onSeats={actions.onSeats}
              view={screen.view}
            />
          ) : null}
          {screen._tag === "Games" ? (
            <PicoGamesFound
              onContinue={actions.onContinue}
              onDismissProblem={actions.onDismissProblem}
              onScan={actions.onScanGames}
              view={screen.view}
            />
          ) : null}
          {screen._tag === "Play" ? (
            <PicoSetupDone facts={screen.facts} finishing={screen.finishing} onFinish={actions.onFinish} />
          ) : null}
        </div>
      </div>
      {screen._tag === "Identity" && screen.backup !== undefined ? (
        <PicoIdentityDialog
          action={PICO_IDENTITY_BACKUP_ACTION}
          form={{ ...IDENTITY_FORM, password: screen.backup.password }}
          identity={screen.backup.identity}
          onClose={actions.backup.onClose}
          onConfirmed={() => undefined}
          onDisposition={() => undefined}
          onEdit={(field, value) => {
            if (field === "password") actions.backup.onEdit(value)
          }}
          onSubmit={actions.backup.onSubmit}
          qr={screen.backup.qr}
        />
      ) : null}
    </PicoScreenShell>
  )
}
