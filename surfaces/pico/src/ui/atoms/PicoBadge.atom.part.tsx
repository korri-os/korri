import { PicoBadge } from "./PicoBadge"

export const name = "Badge"
export const note = "A condition as a pill; tone is a role the screen already has"

export default function PicoBadgePart() {
  return <PicoBadge text="SAVING" tone="info" />
}

export function Resumable() {
  return <PicoBadge text="RESUME" tone="ok" />
}

export function Warning() {
  return <PicoBadge text="FAILED" tone="warn" />
}
