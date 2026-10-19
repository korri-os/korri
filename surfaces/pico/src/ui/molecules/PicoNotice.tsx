import "./PicoNotice.css"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "./PicoCard"

/**
 * Whether a notice reports a normal condition, a failure, or asks something.
 * Three tones, because Korri distinguishes exactly three: a catalog that is
 * merely empty or loading, one that failed, and a question Korri needs
 * answered before it acts (a download it wants a yes for).
 */
export type PicoNoticeTone = "info" | "warn" | "ask"

export interface PicoNoticeAction {
  readonly label: string
  /** Present but inert, as the treaty asks: drawn dimmed rather than hidden. */
  readonly disabled?: boolean
  readonly onPress: () => void
}

/**
 * What the screen says when it has nothing else to show.
 *
 * Loading, empty, failed and asked are the same view of the same data, so they
 * are one component with a tone — a blue card that tells, a red one that
 * warns, a yellow one that asks — not four near-identical screens that drift.
 * The message is Korri's own user-facing copy; Pico never interprets a
 * failure or writes a friendlier version of it.
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
  /** Absent when Korri gave a kicker and nothing more to say under it. */
  readonly message?: string
  readonly tone: PicoNoticeTone
  readonly actions?: readonly PicoNoticeAction[]
}) {
  return (
    <section aria-label={kicker} className="pico-notice">
      <PicoCard
        kicker={title === undefined ? undefined : kicker}
        title={title ?? kicker}
        tone={tone === "warn" ? "warn" : tone === "ask" ? "ask" : "tell"}
      >
        {message === undefined ? null : <p className="pico-notice-message">{message}</p>}
        {actions.length === 0 ? null : (
          <div className="pico-notice-actions">
            {actions.map((action) => (
              <PicoRow disabled={action.disabled} key={action.label} label={action.label} onPress={action.onPress} />
            ))}
          </div>
        )}
      </PicoCard>
    </section>
  )
}
