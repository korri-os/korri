import { useState } from "react"
import { PicoChoicePicker } from "./PicoChoicePicker"

export const name = "Choice Picker"
export const note = "Choose one of Korri's values for a setting with none chosen yet: Add setting"

const SETTINGS = [
  { value: "frameskip", label: "Frameskip" },
  { value: "color", label: "Colour correction" },
  { value: "gb.solar", label: "Solar sensor" },
  { value: "lang", label: "Language" },
]

export default function PicoChoicePickerPart() {
  return <PicoChoicePicker description="4 more settings this runner offers." onCancel={() => undefined} onChoose={() => undefined} options={SETTINGS} title="Add setting" />
}

/** The chosen value, kept in the part, so the list answers a press. */
export function Choosing() {
  const [chosen, setChosen] = useState<string>()
  return (
    <PicoChoicePicker
      description={chosen === undefined ? "Nothing chosen yet." : `Chose ${chosen}.`}
      onCancel={() => setChosen(undefined)}
      onChoose={setChosen}
      options={SETTINGS}
      title="Add setting"
    />
  )
}

export function One() {
  return <PicoChoicePicker onCancel={() => undefined} onChoose={() => undefined} options={[{ value: "bios", label: "Use BIOS" }]} title="Add setting" />
}

/** More values than the screen holds, some long and one with no break point. */
export function Many() {
  return (
    <PicoChoicePicker
      onCancel={() => undefined}
      onChoose={() => undefined}
      options={[
        ...SETTINGS,
        ...Array.from({ length: 14 }, (_, index) => ({ value: `core_option_${index}`, label: `Core option ${index + 1}` })),
        { value: "long", label: "Interframe blending for games that flicker sprites on alternate frames" },
        { value: "word", label: "mgba_gb_colorization_hardware_palette_override_superlongname" },
      ]}
      title="Add setting"
    />
  )
}
