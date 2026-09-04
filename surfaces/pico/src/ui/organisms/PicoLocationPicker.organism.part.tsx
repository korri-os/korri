import { PicoLocationPicker } from "./PicoLocationPicker"

export const name = "Location Picker"
export const note = "Shown only when Korri says there is a real choice"

export default function PicoLocationPickerPart() {
  return (
    <PicoLocationPicker
      locations={[
        { id: "local", label: "This device" },
        { id: "zao", label: "zao" },
      ]}
      onChoose={() => undefined}
      title="Hollow Knight"
    />
  )
}
