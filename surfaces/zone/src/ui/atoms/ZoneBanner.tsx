import "./ZoneBanner.css"

/**
 * The title banner: Mania's blue hexagon with a white band through its
 * middle and the title in yellow, outlined and extruded in black. Used once
 * on a screen, for the current tab. Sized by the type it stands in, times
 * the banner scale, so it rises above the tabs beside it.
 */
export function ZoneBanner({ children }: { readonly children: string }) {
  return (
    <span className="zone-banner">
      <i className="zone-banner-band" aria-hidden="true" />
      <span className="zone-banner-caps">{children}</span>
    </span>
  )
}
