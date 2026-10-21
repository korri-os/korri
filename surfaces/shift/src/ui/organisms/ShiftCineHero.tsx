import { AnimatePresence, motion } from "framer-motion"
import {
  isLaunchInProgress,
  type LaunchStatusView,
} from "../../launch-failure-copy"
import type { ShiftCinematicGame } from "../../pages/ShiftCinematicHome"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"
import { ShiftCineChip } from "../atoms/ShiftCineChip"
import { ShiftCineKicker } from "../atoms/ShiftCineKicker"
import { ShiftCineLoading } from "../atoms/ShiftCineLoading"
import { ShiftCineTitle } from "../atoms/ShiftCineTitle"
import { ShiftCineChips } from "../molecules/ShiftCineChips"

/**
 * The cinematic hero — the focused game's kicker, title, and glanceable copy. It
 * morphs in place with the launch lifecycle: a live `status` swaps the browsing
 * chips for a status kicker plus a loading bar (launching) or a calm reason chip
 * (failure/unavailable). Springs in on change (keyed by game + tone) and carries
 * the polite/assertive live-region semantics for the status announcement.
 */
export function ShiftCineHero({
  game,
  status,
  resuming,
}: {
  readonly game: ShiftCinematicGame
  readonly status: LaunchStatusView | null
  readonly resuming: boolean
}) {
  return (
    // Keep the cinematic crossfade/lift, but do not use `mode="wait"` here:
    // rapid rail focus changes must be latest-state-wins instead of queueing
    // outgoing titles behind old exit animations. The stack wrapper grid-stacks
    // the outgoing/incoming hero layers into one cell so they crossfade in
    // place instead of briefly flowing as two side-by-side columns.
    <div
      className="shift-cine-hero-stack"
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.hero, game.id)}
    >
      <AnimatePresence>
        <motion.div
          key={`${game.id}:${status?.tone ?? "live"}`}
          className="shift-cine-hero"
          role={status ? "status" : undefined}
          aria-live={
            status?.tone === "failed"
              ? "assertive"
              : status
                ? "polite"
                : undefined
          }
          initial={{ opacity: 0, y: 22 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -14 }}
          transition={{ duration: 0.32, ease: "easeOut" }}
        >
          {status ? (
            <>
              <ShiftCineKicker tone={status.tone}>
                {status.kicker}
              </ShiftCineKicker>
              <ShiftCineTitle>{game.title}</ShiftCineTitle>
              {isLaunchInProgress(status.tone) ? (
                <>
                  <ShiftCineLoading />
                  {status.reason ? (
                    <div className="shift-cine-chips">
                      {/* Neutral progress line (e.g. the provider's stage), not
                          the red failure chip. */}
                      <ShiftCineChip>{status.reason}</ShiftCineChip>
                    </div>
                  ) : null}
                </>
              ) : status.reason ? (
                <div className="shift-cine-chips">
                  <ShiftCineChip tone="reason">{status.reason}</ShiftCineChip>
                </div>
              ) : null}
            </>
          ) : (
            <>
              <ShiftCineKicker>
                {resuming ? "Continue playing" : "Ready to play"}
              </ShiftCineKicker>
              <ShiftCineTitle>{game.title}</ShiftCineTitle>
              <ShiftCineChips
                subtitle={game.subtitle}
                genre={game.genre}
                developer={game.developer}
                lastPlayedLabel={game.lastPlayedLabel}
                playtimeLabel={game.playtimeLabel}
                favorite={game.favorite}
                fresh={game.fresh}
                download={game.download}
                saves={game.saves}
              />
            </>
          )}
        </motion.div>
      </AnimatePresence>
    </div>
  )
}
