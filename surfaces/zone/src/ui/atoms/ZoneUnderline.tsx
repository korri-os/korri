import "./ZoneUnderline.css"

/**
 * The lit mark: a pink parallelogram the size of the panel it sits in, one
 * drop edge down and to the left, behind the panel's body. What shows is a
 * bar along the bottom and a sliver along the leading slant, as under
 * Mania's YES. It is a child of a ZonePanel and takes the panel's shape
 * through `--zone-panel-shape`; alone, it is the shape at its box's size.
 */
export function ZoneUnderline() {
  return <i className="zone-underline" aria-hidden="true" />
}
