import { PicoCoverArt } from "./PicoCoverArt"

export const name = "Cover Art"
export const note = "Falls back to title initials when Korri has no art"

export default function PicoCoverArtPart() {
  return <PicoCoverArt title="Celeste Classic" />
}
