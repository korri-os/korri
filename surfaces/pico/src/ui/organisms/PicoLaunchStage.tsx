/**
 * What the screen shows while Korri is doing something with a game.
 *
 * Deliberately empty of facts. This is on screen for the seconds between a
 * press and a game appearing, and Korri publishes no percentage — so the bar
 * here is a barber pole that moves without ever filling, which says "working"
 * without claiming progress. The kicker is Korri's own headline; the detail
 * line is shown only when there is one.
 */
export function PicoLaunchStage({
  kicker,
  detail,
  gameTitle,
}: {
  readonly kicker: string
  readonly detail?: string
  readonly gameTitle?: string
}) {
  return (
    <section aria-live="polite" className="pico-launch-stage">
      <h1 className="pico-launch-stage-kicker">{kicker}</h1>
      {gameTitle === undefined ? null : (
        <p className="pico-launch-stage-game">{gameTitle}</p>
      )}
      {detail === undefined ? null : (
        <p className="pico-launch-stage-detail">{detail}</p>
      )}
      <span aria-hidden className="pico-launch-stage-bar" />
    </section>
  )
}
