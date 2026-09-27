import { PicoTitle } from "./PicoTitle"

export const name = "Title"
export const note = "Display face; a step on the ramp, never a stated size"

export default function PicoTitlePart() {
  return <PicoTitle size="lg" text="HOLLOW KNIGHT" />
}

export function LongTitle() {
  return <PicoTitle size="xl" text="THE LEGEND OF ZELDA: A LINK TO THE PAST" />
}

export function AccentHeading() {
  return <PicoTitle level={2} size="md" text="LAST PLAYED" tone="accent" />
}

export function WarningHeading() {
  return <PicoTitle level={3} size="sm" text="UNSAVED PROGRESS" tone="warn" />
}
