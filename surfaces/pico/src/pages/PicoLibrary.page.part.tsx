import { fixtureModel } from "../fixtures/fixture-host"
import { PICO_ALL_SECTIONS, picoLibraryViewFrom } from "../pico-library-view"
import { PicoLibrary } from "./PicoLibrary"

export const name = "Find"
export const note = "Search and collections, both computed from the catalog Korri already sent"

export default function PicoLibraryPagePart() {
  return (
    <PicoLibrary
      clockLabel={fixtureModel.clockLabel}
      library={picoLibraryViewFrom(fixtureModel.catalog, "", PICO_ALL_SECTIONS)}
      onBackspace={() => undefined}
      onClear={() => undefined}
      onOpen={() => undefined}
      onSection={() => undefined}
      onType={() => undefined}
      section={PICO_ALL_SECTIONS}
    />
  )
}
