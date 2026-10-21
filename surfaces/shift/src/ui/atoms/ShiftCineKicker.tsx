import type { ReactNode } from "react"
import type { LaunchStatusTone } from "../../launch-failure-copy"

/** The small overline above the hero title ("Ready to play", "Couldn't start").
 * `tone` mirrors the launch lifecycle so the colour reflects state; omit it for
 * the neutral browsing kicker. `attention` marks a question the person must
 * answer that is not a failure (a save split, an in-use warning). */
export function ShiftCineKicker({
  tone,
  children,
}: {
  readonly tone?: LaunchStatusTone | "attention"
  readonly children: ReactNode
}) {
  return (
    <span className="shift-cine-kicker" data-tone={tone}>
      {children}
    </span>
  )
}
