import "./ZoneTagRow.css"
import { useRef } from "react"
import { useZoneFit } from "../../use-zone-fit"
import { ZoneTag, type ZoneTagItem, type ZoneTagJoin } from "../atoms/ZoneTag"

const joinOf = (index: number, count: number): ZoneTagJoin =>
  count === 1 ? "only" : index === 0 ? "first" : index === count - 1 ? "last" : "middle"

/**
 * The row of tags under a card's diagonal band, cut on slants as the TIME
 * ATTACK card's MANIA / ACT 1 / SONIC strip. Every tag is kept whatever the
 * width: the row's caps shrink first (to half), then a tag truncates.
 *
 * The row's height is set by its base type, not the fitted type, so
 * shrinking the caps never moves the card's layout.
 */
export function ZoneTagRow({ tags }: { readonly tags: readonly ZoneTagItem[] }) {
  const row = useRef<HTMLSpanElement>(null)
  useZoneFit(row, [row], tags.map(tag => tag.label).join("\n"))
  return (
    <span className="zone-tag-row" ref={row}>
      {tags.map((tag, index) => (
        <ZoneTag join={joinOf(index, tags.length)} key={`${tag.kind}:${tag.label}`} kind={tag.kind} label={tag.label} />
      ))}
    </span>
  )
}
