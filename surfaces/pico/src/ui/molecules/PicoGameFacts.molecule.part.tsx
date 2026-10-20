import { PicoGameFacts } from "./PicoGameFacts"

export const name = "Game Facts"
export const note = "Title, provenance and play chips — only what Korri published"

export default function PicoGameFactsPart() {
  return (
    <PicoGameFacts
      kicker="LAST PLAYED"
      resumable
      stats={[
        { figure: "3", caption: "PLAYS" },
        { figure: "2H 10M", caption: "PLAYED" },
      ]}
      subtitle="Switch · This device"
      title="Hollow Knight"
    />
  )
}

export function NeverPlayed() {
  return <PicoGameFacts stats={[]} title="Petal Quest" />
}

export function LongTitle() {
  return (
    <PicoGameFacts
      level={2}
      stats={[{ figure: "12345", caption: "PLAYS" }]}
      subtitle="Super Nintendo Entertainment System · Living room device"
      title="The Legend of Zelda: A Link to the Past"
    />
  )
}

/** Playing fetches a runtime first; the amount is a chip beside the play facts. */
export function NeedsDownload() {
  return (
    <PicoGameFacts
      download="1.5 GB DOWNLOAD"
      stats={[{ figure: "2", caption: "PLAYS" }]}
      subtitle="Steam · This device"
      title="Hollow Knight"
    />
  )
}

export function NeedsDownloadUnknownSize() {
  return <PicoGameFacts download="DOWNLOAD NEEDED" resumable stats={[]} subtitle="Steam · This device" title="Celeste" />
}

/** Waiting behind another game's download. */
export function DownloadQueued() {
  return <PicoGameFacts download="QUEUED" stats={[]} subtitle="This device" title="Mega Man Rock N Roll" />
}

/** Downloading in the background. */
export function Downloading() {
  return <PicoGameFacts download="DOWNLOADING 41%" stats={[]} subtitle="This device" title="Mega Man Rock N Roll" />
}
