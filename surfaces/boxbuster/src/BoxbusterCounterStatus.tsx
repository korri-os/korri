/**
 * Where a launch stands, at the counter.
 *
 * Work under way and a running game are a line above the tape: information,
 * no decision. A problem is the one status that asks for an answer, so it
 * takes the tape's place and the focus.
 */
import "./BoxbusterCounterStatus.css"
import type { CounterStatus } from "./boxbuster-store-view"

/** The line for work under way or a running game. Nothing otherwise. */
export function BoxbusterCounterStatusLine({
  status,
}: {
  status: CounterStatus
}) {
  if (status._tag !== "Working" && status._tag !== "Playing") return null
  return (
    <div className="boxbuster-counter-status" role="status">
      <p className="boxbuster-counter-status-kicker">{status.kicker}</p>
      {status._tag === "Working" && status.detail !== undefined ? (
        <p className="boxbuster-counter-status-detail">{status.detail}</p>
      ) : null}
    </div>
  )
}

export function BoxbusterCounterProblem({
  problem,
  onRetry,
  onDismiss,
}: {
  problem: Extract<CounterStatus, { _tag: "Problem" }>
  onRetry: () => void
  onDismiss: () => void
}) {
  return (
    <section className="boxbuster-counter-problem" role="alert">
      <h2 className="boxbuster-counter-problem-kicker">{problem.kicker}</h2>
      {problem.title === undefined ? null : (
        <p className="boxbuster-counter-problem-title">{problem.title}</p>
      )}
      <p className="boxbuster-counter-problem-reason">{problem.reason}</p>
      <div className="boxbuster-counter-problem-actions">
        {problem.canRetry ? (
          <button
            type="button"
            className="boxbuster-counter-problem-action"
            data-focus-home=""
            onClick={onRetry}
          >
            Try again
          </button>
        ) : null}
        <button
          type="button"
          className="boxbuster-counter-problem-action"
          {...(problem.canRetry ? {} : { "data-focus-home": "" })}
          onClick={onDismiss}
        >
          Back to the shelves
        </button>
      </div>
    </section>
  )
}
