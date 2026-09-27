import type { SurfaceCatalog } from "@contracts/surface/korri-surface"
import { fixtureModel } from "../fixtures/fixture-host"
import { PICO_ALL_SECTIONS, picoLibraryViewFrom, type PicoOrder } from "../pico-library-view"
import { PicoLibrary } from "./PicoLibrary"

export const name = "Find"
export const note = "Search and collections, both computed from the catalog Korri already sent"

function library(query: string, section = PICO_ALL_SECTIONS, order: PicoOrder = "korri", catalog: SurfaceCatalog = fixtureModel.catalog) {
  return <PicoLibrary clockLabel={fixtureModel.clockLabel}
    library={picoLibraryViewFrom(catalog, query, section, order)} section={section} order={order}
    onBackspace={() => undefined} onOrder={() => undefined} onClear={() => undefined}
    onOpen={() => undefined} onSection={() => undefined} onType={() => undefined} />
}

export function SearchResults() { return library("Switch") }
export function NoResults() { return library("No such cart") }
// Loading and Error also convert to empty results: this page has no separate notice for them.
export function Empty() { return library("", PICO_ALL_SECTIONS, "korri", { _tag: "Empty" }) }
export function Collection() { return library("", "zao") }
export function Alphabetical() { return library("", PICO_ALL_SECTIONS, "title") }
export function MostPlayed() { return library("", PICO_ALL_SECTIONS, "played") }
export function Recent() { return library("", PICO_ALL_SECTIONS, "recent") }

export default function PicoLibraryPagePart() {
  return (
    <PicoLibrary
      clockLabel={fixtureModel.clockLabel}
      library={picoLibraryViewFrom(fixtureModel.catalog, "", PICO_ALL_SECTIONS)}
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
