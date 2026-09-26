import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "./PicoCard"

/**
 * Whether a notice reports a normal condition or a failure. Two tones, because
 * Korri distinguishes exactly two: a catalog that is merely empty or loading,
 * and one that failed.
 */
export type PicoNoticeTone = "info" | "warn"

export interface PicoNoticeAction {
  readonly label: string
  readonly onPress: () => void
}

/**
 * What the screen says when it has nothing else to show.
 *
 * Loading, empty, and failed are the same view of the same data, so they are
 * one component with a tone — a blue card that tells, or a red one that warns
 * — not three near-identical screens that drift. The message is Korri's own
 * user-facing copy; Pico never interprets a failure or writes a friendlier
 * version of it.
 *
 * Actions arrive as a list rather than as named optional callbacks: a failure
 * may offer a retry, a dismissal, both, or neither. An empty list is a notice
 * that simply states a fact.
 */
export function PicoNotice({
  kicker,
  title,
  message,
  tone,
  actions = [],
}: {
  readonly kicker: string
  /** What the notice is about, when it is about one thing (a game). Without
   * one, the kicker is the heading. */
  readonly title?: string
  readonly message: string
  readonly tone: PicoNoticeTone
  readonly actions?: readonly PicoNoticeAction[]
}) {
  return (
    <section aria-label={kicker} className="pico-notice">
      <PicoCard
        kicker={title === undefined ? undefined : kicker}
        title={title ?? kicker}
        tone={tone === "warn" ? "warn" : "tell"}
      >
        <p className="pico-notice-message">{message}</p>
        {actions.length === 0 ? null : (
          <div className="pico-notice-actions">
            {actions.map((action) => (
              <PicoRow key={action.label} label={action.label} onPress={action.onPress} />
            ))}
          </div>
        )}
      </PicoCard>
    </section>
  )
}
