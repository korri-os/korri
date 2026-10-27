import "../../pico-motion.css"
import "./PicoNotice.css"
import { useEffect, useRef } from "react"
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
  /** The cursor starts here: the answer that declines, when Korri names one,
   * so a stray press never agrees. */
  readonly start?: boolean
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
 *
 * When Korri names the answer that declines (`start`), the cursor starts on
 * it, as a question over Settings starts on Cancel: a stray press never
 * agrees, such as to a plugin install and its permissions.
 *
 * A message of several lines is a report to read before answering, such as
 * what a plugin asks for before it installs. As in a question over Settings,
 * each line is then a stop for the cursor. With no answer to start on, the
 * cursor starts on the first line, so a stray press answers nothing: a line
 * is a statement, and a press on it does nothing. The answers stand between
 * the title and the report, so they never leave the screen: a report longer
 * than its room scrolls under them, and the d-pad walks it straight down from
 * the last answer, because a focus-driven screen scrolls only by moving focus.
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
  const lines = message === undefined ? [] : message.split("\n").map(line => line.trim()).filter(line => line !== "")
  const report = lines.length > 1
  const noticeRef = useRef<HTMLElement>(null)
  const start = actions.findIndex(action => action.start === true)
  // The cursor starts on the answer that declines, else on the report's first
  // line, unless it is already on the screen: a notice never takes focus from
  // a control the person holds.
  useEffect(() => {
    const active = document.activeElement
    if (active !== null && active !== document.body && active.isConnected) return
    const notice = noticeRef.current
    const answer = start < 0 ? undefined : notice?.querySelectorAll<HTMLElement>(".pico-notice-actions > button")[start]
    ;(answer ?? (report ? notice?.querySelector<HTMLElement>(".pico-notice-line") : undefined))?.focus()
  }, [report, start])
  const answers = actions.length === 0 ? null : (
    <div className="pico-notice-actions">
      {actions.map((action) => (
        <PicoRow disabled={action.disabled} key={action.label} label={action.label} onPress={action.onPress} />
      ))}
    </div>
  )
  const card = (
    <PicoCard
      kicker={title === undefined ? undefined : kicker}
      title={title ?? kicker}
      tone={tone === "warn" ? "warn" : tone === "ask" ? "ask" : "tell"}
    >
      {report ? answers : null}
      {report ? (
        <ul className="pico-notice-report">
          {/* Lines may repeat; a line's place in the report is its identity. */}
          {lines.map((line, index) => (
            <li className="pico-notice-line" key={`${index}:${line}`} tabIndex={0}>{line}</li>
          ))}
        </ul>
      ) : message === undefined ? null : <p className="pico-notice-message">{message}</p>}
      {report ? null : answers}
    </PicoCard>
  )
  return (
    <section aria-label={kicker} className="pico-notice" ref={noticeRef}
      data-report={!report ? undefined : title === undefined ? "after-title" : "after-kicker"}>
      {/* A report measures its room, so a short screen can make the card fit
          before it would push the answers off it. */}
      {report ? <div className="pico-notice-fit">{card}</div> : card}
    </section>
  )
}
