import { PicoSub } from "../atoms/PicoSub"
import { PicoTitle } from "../atoms/PicoTitle"
import { PicoCart } from "./PicoCart"

/**
 * The top of a game's screen: its cart as a still, its title, and where it
 * came from. Says nothing about play — that is the body's, and a head that
 * also carried stats would have to be re-laid every time the body changed.
 */
export function PicoDetailHead({
  id,
  title,
  subtitle,
  artUrl,
}: {
  readonly id: string
  readonly title: string
  readonly subtitle?: string
  readonly artUrl?: string
}) {
  return (
    <header className="pico-detail-head">
      <div className="pico-detail-head-art">
        <PicoCart artUrl={artUrl} id={id} placement="still" title={title} />
      </div>
      <div className="pico-detail-head-info">
        <PicoTitle size="xl" text={title} />
        {subtitle === undefined ? null : <PicoSub text={subtitle} />}
      </div>
    </header>
  )
}
