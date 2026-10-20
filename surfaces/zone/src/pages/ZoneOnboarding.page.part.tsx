import type { SurfaceModel } from "@contracts/surface/korri-surface"
import { useRef, useSyncExternalStore } from "react"
import {
  controllerPad,
  controllerPadWaiting,
  controllersFour,
  networkOnline,
  onboardingModel,
} from "../fixtures/fixture-layers"
import { canGoOn, createOnboardingProgram, deviceNameSetting } from "../fixtures/layer-program"
import { ZonePartFrame } from "../fixtures/ZonePartFrame"
import { plan } from "../plan/zone-select"
import type { ZoneBand, ZoneStrips } from "../ui/templates/ZoneShell"
import { ZoneOnboarding, type ZoneOnboardingStepId } from "./ZoneOnboarding"

export const name = "Onboarding"
export const note = "First boot: controllers, Wi-Fi, the device name, a new save, under a strip of four steps"

export const composition = {
  default: [
    { part: "src/ui/organisms/ZoneControllerCard.organism.part.tsx", state: "default" },
    { part: "src/ui/molecules/ZoneTagRow.molecule.part.tsx", state: "default" },
  ],
  OneHeard: [{ part: "src/ui/organisms/ZoneControllerCard.organism.part.tsx", state: "Heard" }],
  FourControllers: [
    { part: "src/ui/organisms/ZoneControllerCard.organism.part.tsx", state: "Heard" },
    { part: "src/ui/organisms/ZoneControllerCard.organism.part.tsx", state: "SecondSeat" },
    { part: "src/ui/organisms/ZoneControllerCard.organism.part.tsx", state: "default" },
  ],
  WifiStep: [{ part: "src/ui/molecules/ZoneOptionGroup.molecule.part.tsx", state: "default" }],
  NameStep: [{ part: "src/ui/molecules/ZoneOptionRow.molecule.part.tsx", state: "Text" }],
  NameTyping: [{ part: "src/ui/molecules/ZoneKeyboard.molecule.part.tsx", state: "default" }],
  IdentityStep: [{ part: "src/ui/molecules/ZoneSaveCard.molecule.part.tsx", state: "Focused" }],
  Finish: [{ part: "src/ui/molecules/ZoneSaveCard.molecule.part.tsx", state: "WithText" }],
} as const

/** Unexported: the real page on the real onboarding model, a pretend Korri joining and saving. */
function Scenario({
  korri,
  step = "controllers",
  started = false,
  typing = false,
  strips,
  band,
}: {
  readonly korri: SurfaceModel
  readonly step?: ZoneOnboardingStepId
  readonly started?: boolean
  /** Start on the NAME step with the keyboard up. */
  readonly typing?: boolean
  readonly strips?: ZoneStrips
  readonly band?: ZoneBand
}) {
  const program = useRef<ReturnType<typeof createOnboardingProgram> | null>(null)
  program.current ??= createOnboardingProgram(korri, {
    step,
    started,
    // An empty cursor lands on the step's first row, as opening the step does.
    focus: started ? "finish" : step === "controllers" ? "next" : "",
    entry: typing ? { draft: "mini", caret: 4, layer: "letters", focusedKey: "i" } : undefined,
  })
  const { dispatch, subscribe, get } = program.current
  const { model, context } = useSyncExternalStore(subscribe, get)
  // The binding hands the page the plan's two answers for the screen; the
  // part asks the plan for the frame's device.
  const layout = plan(window.innerWidth, window.innerHeight)
  return (
    <ZoneOnboarding
      band={band ?? layout.band}
      canGoOn={canGoOn(model.step, context)}
      controllers={context.onboarding?.controllers ?? []}
      entry={model.entry}
      finishing={model.finishing}
      focus={model.focus}
      nameSetting={deviceNameSetting(context)}
      network={context.network}
      onFinish={() => dispatch({ _tag: "ChoseFinish" })}
      onFocusTarget={(target) => dispatch({ _tag: "FocusedTarget", target })}
      onJoin={(networkId) => dispatch({ _tag: "OnboardingJoin", networkId })}
      onKey={(key) => dispatch({ _tag: "PressedKey", key })}
      onNext={() => dispatch({ _tag: "ChoseNext" })}
      onOpenName={() => dispatch({ _tag: "OpenedName" })}
      onScan={() => dispatch({ _tag: "ChoseScan" })}
      onSkip={() => dispatch({ _tag: "ChoseSkip" })}
      onStart={() => dispatch({ _tag: "ChoseStart" })}
      settingsStatus={context.settingsStatus}
      started={model.started}
      step={model.step}
      strips={strips ?? layout.strips}
    />
  )
}

/** One controller Korri sees but has not heard: WAITING, NEXT dim. */
export default function Waiting() {
  return <Scenario korri={onboardingModel([controllerPadWaiting])} />
}

/** It has been heard: its name and 1P, and NEXT lights. */
export function OneHeard() {
  return <Scenario korri={onboardingModel([controllerPad])} />
}

/** Four controllers, three heard: 1P, 2P, 3P and one still WAITING. */
export function FourControllers() {
  return <Scenario korri={onboardingModel(controllersFour)} />
}

/** Wi-Fi: offline, three networks, SCAN and SKIP. A on a saved or open network joins it. */
export function WifiStep() {
  return <Scenario korri={onboardingModel([controllerPad])} step="wifi" />
}

/** The device's name as an OPTIONS row; A raises the keyboard. */
export function NameStep() {
  return <Scenario korri={onboardingModel([controllerPad], networkOnline)} step="name" />
}

/** The keyboard up over the NAME step; DONE saves the name. */
export function NameTyping() {
  return <Scenario korri={onboardingModel([controllerPad], networkOnline)} step="name" typing />
}

/** The NEW SAVE card, START in its well; FINISH waits for START. */
export function IdentityStep() {
  return <Scenario korri={onboardingModel([controllerPad], networkOnline)} step="identity" />
}

/** START chosen: the card reads READY and FINISH is lit. */
export function Finish() {
  return <Scenario korri={onboardingModel([controllerPad], networkOnline)} started step="identity" />
}

/** A narrow column with four controllers: the cards wrap, NEXT stays on screen. */
export function Narrow() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <Scenario band="narrow" korri={onboardingModel(controllersFour)} strips="short" />
    </ZonePartFrame>
  )
}
