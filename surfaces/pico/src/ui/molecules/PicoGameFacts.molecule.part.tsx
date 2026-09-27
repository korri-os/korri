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
