import "./ZoneCaps.css"

/**
 * Where the caps take their size from. `inherit` reads the strip the caps
 * stand in (the shell sets a type size on each strip); the named sizes read
 * the scale directly for caps that stand alone.
 */
export type ZoneCapsSize = "inherit" | "tab" | "hint" | "button" | "foot"

/**
 * White chunky caps: Anton, upper case, a black outline all round and the
 * blue edge down and right. On a light panel the panel removes the edge
 * through `--zone-caps-shadow`. The one lettering in Zone; every word is a
 * verb or a label, so there is no wrapping and no lower case.
 */
export function ZoneCaps({
  size = "inherit",
  children,
}: {
  readonly size?: ZoneCapsSize
  readonly children: string
}) {
  return (
    <span className="zone-caps" data-size={size}>
      {children}
    </span>
  )
}
