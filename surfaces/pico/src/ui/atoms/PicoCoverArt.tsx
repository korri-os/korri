import { picoInitials } from "../../pico-initials"

/**
 * A game's cover, or an honest stand-in when Korri has none.
 *
 * The treaty leaves `coverArtUrl` absent rather than inventing a placeholder
 * asset, so presenting the gap is the surface's job: initials on the game's own
 * accent, which reads as a deliberate cartridge label rather than a broken
 * image.
 */
export function PicoCoverArt({
  title,
  artUrl,
}: {
  readonly title: string
  readonly artUrl?: string
}) {
  if (artUrl === undefined || artUrl === "") {
    return (
      <span aria-hidden className="pico-cover-art-initials">
        {picoInitials(title)}
      </span>
    )
  }
  return <img alt="" className="pico-cover-art-image" src={artUrl} />
}
