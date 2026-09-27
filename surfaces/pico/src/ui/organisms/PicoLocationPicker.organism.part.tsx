import { fixtureGame, longTitleGame } from "../../fixtures/named-states"
import { PicoLocationPicker } from "./PicoLocationPicker"

export function LongTitle() {
  return <PicoLocationPicker title={longTitleGame.title} locations={fixtureGame("tetris").launchLocations!} onChoose={() => undefined} />
}

// Empty and single-location input are not product states: the caller opens
// this picker only when the game has a real launch choice.
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
