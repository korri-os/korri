import { PicoSetupHeading } from "./PicoSetupHeading"

export const name = "Setup Heading"
export const note = "A setup screen's title in the accent over a floor rule, and a sentence under it"

export default function PicoSetupHeadingPart() {
  return <PicoSetupHeading message="Your identity is the key that ties your Korri devices together." title="Who are you?" />
}

export function TitleOnly() {
  return <PicoSetupHeading title="One moment" />
}
