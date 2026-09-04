import { PicoButton } from "../atoms/PicoButton"

/**
 * Whether a notice is reporting a normal condition or a failure. Two tones,
 * because Korri distinguishes exactly two: a catalog that is merely empty or
 * loading, and one that failed.
 */
export type PicoNoticeTone = "info" | "warn"

/**
 * What the screen says when it has no shelf to show.
 *
 * Loading, empty, and failed are the same view of the same data, so they are
 * one component with a tone — not three near-identical screens that drift.
 * The message is Korri's own user-facing copy; Pico never interprets a failure
 * or writes a friendlier version of it.
 */
export function PicoNotice({
  kicker,
  message,
  tone,
  retryLabel,
  onRetry,
}: {
  readonly kicker: string
  readonly message: string
  readonly tone: PicoNoticeTone
  readonly retryLabel?: string
  readonly onRetry?: () => void
}) {
  return (
    <section className="pico-notice" data-tone={tone}>
      <h1 className="pico-notice-kicker">{kicker}</h1>
      <p className="pico-notice-message">{message}</p>
      {onRetry === undefined || retryLabel === undefined ? null : (
        <PicoButton label={retryLabel} onPress={onRetry} />
      )}
    </section>
  )
}
