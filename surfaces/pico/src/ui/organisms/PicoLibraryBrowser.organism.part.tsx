import { fixtureModel } from "../../fixtures/fixture-host"
import { PICO_ALL_SECTIONS, picoLibraryViewFrom } from "../../pico-library-view"
import { PicoLibraryBrowser } from "./PicoLibraryBrowser"

export const name = "Library Browser"
export const note = "Results stay visible while typing; hiding them means pressing keys blind"

export default function PicoLibraryBrowserPart() {
  return (
    <PicoLibraryBrowser
      library={picoLibraryViewFrom(fixtureModel.catalog, "SP", PICO_ALL_SECTIONS)}
      onBackspace={() => undefined}
      onClear={() => undefined}
      onOpen={() => undefined}
      onSection={() => undefined}
      onType={() => undefined}
      section={PICO_ALL_SECTIONS}
    />
  )
}
