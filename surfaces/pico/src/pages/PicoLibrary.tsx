import type { PicoInviteNoticeView } from "../pico-invite-view"
import type { PicoLibraryView, PicoOrder } from "../pico-library-view"
import { PicoInviteNotices } from "../ui/organisms/PicoInviteNotices"
import { PicoLibraryBrowser } from "../ui/organisms/PicoLibraryBrowser"
import { PicoScreenShell } from "../ui/templates/PicoScreenShell"

const HINTS = [
  { hintKey: "a", label: "OPEN" },
  { hintKey: "b", label: "BACK" },
] as const

/** Finding a game, in the same shell as everything else. An invite that
 * arrives while the person looks is announced over it, as on home. */
export function PicoLibrary({
  library,
  section,
  order,
  onOrder,
  onType,
  onBackspace,
  onClear,
  onSection,
  onOpen,
  notices = [],
  clockLabel,
}: {
  readonly library: PicoLibraryView
  readonly section: string
  readonly order: PicoOrder
  readonly onOrder: (order: PicoOrder) => void
  readonly onType: (character: string) => void
  readonly onBackspace: () => void
  readonly onClear: () => void
  readonly onSection: (section: string) => void
  readonly onOpen: (gameId: string) => void
  /** Invites announced now, newest first. */
  readonly notices?: readonly PicoInviteNoticeView[]
  readonly clockLabel?: string
}) {
  return (
    <PicoScreenShell clockLabel={clockLabel} hints={HINTS} label="FIND" over={<PicoInviteNotices notices={notices} />}>
      <PicoLibraryBrowser
        library={library}
        onBackspace={onBackspace}
        onClear={onClear}
        onOpen={onOpen}
        onOrder={onOrder}
        onSection={onSection}
        onType={onType}
        order={order}
        section={section}
      />
    </PicoScreenShell>
  )
}
