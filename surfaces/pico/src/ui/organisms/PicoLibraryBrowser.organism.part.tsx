import { fixtureModel } from "../../fixtures/fixture-host"
import { PICO_ALL_SECTIONS, picoLibraryViewFrom } from "../../pico-library-view"
import { PicoLibraryBrowser } from "./PicoLibraryBrowser"

const callbacks = {
  onBackspace: () => undefined,
  onOrder: () => undefined,
  onClear: () => undefined,
  onOpen: () => undefined,
  onSection: () => undefined,
  onType: () => undefined,
}

export function NoResults() {
  return <PicoLibraryBrowser {...callbacks} library={picoLibraryViewFrom(fixtureModel.catalog, "ZZZZ", PICO_ALL_SECTIONS)} order="korri" section={PICO_ALL_SECTIONS} />
}

export function EmptyLibrary() {
  return <PicoLibraryBrowser {...callbacks} library={picoLibraryViewFrom({ _tag: "Empty" }, "", PICO_ALL_SECTIONS)} order="korri" section={PICO_ALL_SECTIONS} />
}

export function CollectionByTitle() {
  return <PicoLibraryBrowser {...callbacks} library={picoLibraryViewFrom(fixtureModel.catalog, "", "This device", "title")} order="title" section="This device" />
}

export function RecentlyPlayed() {
  return <PicoLibraryBrowser {...callbacks} library={picoLibraryViewFrom(fixtureModel.catalog, "", PICO_ALL_SECTIONS, "recent")} order="recent" section={PICO_ALL_SECTIONS} />
}

// Query/order/section are controlled inputs. These snapshots do not simulate
// host updates. The page part also shows fixed inputs, not a live search loop.
export const name = "Library Browser"
export const note = "Results stay visible while typing; hiding them means pressing keys blind"

export default function PicoLibraryBrowserPart() {
  return (
    <PicoLibraryBrowser
      library={picoLibraryViewFrom(fixtureModel.catalog, "SP", PICO_ALL_SECTIONS)}
      onBackspace={() => undefined}
      onOrder={() => undefined}
      onClear={() => undefined}
      onOpen={() => undefined}
      onSection={() => undefined}
      onType={() => undefined}
      order="korri"
      section={PICO_ALL_SECTIONS}
    />
  )
}
