import { fixtureModel } from "../fixtures/fixture-host"
import { picoSettingsViewFromModel } from "../pico-settings-view"
import { PicoSettings } from "./PicoSettings"

export const name = "Settings"
export const note = "Korri's facts and settings; a destructive action asks first, in Korri's words"

export default function PicoSettingsPagePart() {
  return (
    <PicoSettings
      clockLabel={fixtureModel.clockLabel}
      onAsk={() => undefined}
      onCancel={() => undefined}
      onChange={() => undefined}
      onConfirm={() => undefined}
      onDismissProblem={() => undefined}
      onRun={() => undefined}
      settings={picoSettingsViewFromModel(fixtureModel)}
    />
  )
}
