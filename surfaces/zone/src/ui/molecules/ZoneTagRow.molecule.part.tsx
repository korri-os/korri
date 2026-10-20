import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneTagRow } from "./ZoneTagRow"

export const name = "Tag row"
export const note = "Joined tags on slants: MANIA / ACT 1 / SONIC. Every tag stays; caps shrink first"

export default function Default() {
  return <ZoneTagRow tags={[{ kind: "system", label: "PC" }, { kind: "here", label: "Here" }, { kind: "remote", label: "Zao" }]} />
}

/** One tag: no cut ends. */
export function OneTag() {
  return <ZoneTagRow tags={[{ kind: "system", label: "Switch" }]} />
}

/** Two tags: one cut on the right, one on the left. */
export function TwoTags() {
  return <ZoneTagRow tags={[{ kind: "system", label: "GB" }, { kind: "remote", label: "Zao" }]} />
}

/** Three tags in a side card's width. */
export function ThreeTags() {
  return (
    <ZonePartFrame shape="card">
      <ZoneTagRow tags={[{ kind: "system", label: "PC" }, { kind: "here", label: "Here" }, { kind: "remote", label: "Zao" }]} />
    </ZonePartFrame>
  )
}

/** Four tags in a side card's width: the caps shrink, then the longest truncates. */
export function FourOverflowing() {
  return (
    <ZonePartFrame shape="card">
      <ZoneTagRow
        tags={[
          { kind: "system", label: "Mega Drive" },
          { kind: "here", label: "Here" },
          { kind: "remote", label: "Zao" },
          { kind: "continue", label: "Continue" },
        ]}
      />
    </ZonePartFrame>
  )
}
