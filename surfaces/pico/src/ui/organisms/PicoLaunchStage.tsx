/**
 * What the screen shows while Korri is doing something with a game.
 *
 * Deliberately calm and almost empty. This is on screen for the seconds
 * between a press and a game appearing, and anything busy here — a spinner
 * racing, a progress bar guessing — would be inventing detail Korri has not
 * published. The kicker is Korri's own headline; the detail line is shown only
 * when there is one.
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
    </section>
  )
}
