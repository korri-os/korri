import type { PicoLibraryView } from "../pico-library-view"
import { PicoLibraryBrowser } from "../ui/organisms/PicoLibraryBrowser"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

const HINTS = [
  { hintKey: "a", label: "OPEN" },
  { hintKey: "b", label: "BACK" },
] as const

/** Finding a game, in the same shell as everything else. */
export function PicoLibrary({
  library,
  section,
  onType,
  onBackspace,
  onClear,
  onSection,
  onOpen,
  clockLabel,
}: {
  readonly library: PicoLibraryView
  readonly section: string
  readonly onType: (character: string) => void
  readonly onBackspace: () => void
  readonly onClear: () => void
  readonly onSection: (section: string) => void
  readonly onOpen: (gameId: string) => void
  readonly clockLabel?: string
}) {
  return (
    <PicoScreenShell backdrop="none" clockLabel={clockLabel} hints={HINTS} label="PICO ▸ FIND">
      <PicoLibraryBrowser
        library={library}
        onBackspace={onBackspace}
        onClear={onClear}
        onOpen={onOpen}
        onSection={onSection}
        onType={onType}
        section={section}
      />
    </PicoScreenShell>
  )
}
