import { PicoKeyArt } from "../atoms/PicoKeyArt"

/**
 * Key art laid behind a screen's content, with the scrim that keeps the content
 * legible on it.
 *
 * The art and the scrim travel together because one without the other is a
 * mistake: art with no scrim is legible until someone adds a game with a pale
 * sky in the bottom third, and a scrim with no art is a stripe. The shelf and a
 * game's own screen both stand on this, so it is one component and not a rule
 * each of them restates.
 */
export function PicoKeyArtStage({ src }: { readonly src?: string }) {
  return (
    <div className="pico-key-art-stage">
      <PicoKeyArt src={src} />
    </div>
  )
}
