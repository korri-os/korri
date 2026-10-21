/**
 * One side of a split (ADR 0014): the device that made the save, when its
 * session ended by that device's clock, how long it lasted, and its newest
 * screenshot or savestate thumbnail.
 *
 * The card is a native button. Picking it hands the host's own action id back
 * unchanged; Shift never decides which side wins, and the end time is only
 * shown, never compared. A side without a picture, or whose picture fails to
 * load, says so instead of borrowing the game's box art: box art would look
 * like a picture of that save.
 */
import { ImageOff } from "lucide-react"
import { useState } from "react"
import {
  formatSaveEndedAt,
  formatSavePlayed,
} from "../../save-sync-labels"
import {
  SHIFT_DESIGN_PARTS,
  shiftDesignPartAttrs,
} from "../../shift-design-parts"

export interface ShiftSaveSideCardProps {
  readonly actionId: string
  readonly device: string
  readonly endedAt: number
  readonly durationSeconds: number
  readonly imageUrl?: string
  readonly focused?: boolean
  readonly now?: Date
  readonly onFocus: () => void
  readonly onPick: (actionId: string) => void
}

export function ShiftSaveSideCard({
  actionId,
  device,
  endedAt,
  durationSeconds,
  imageUrl,
  focused,
  now,
  onFocus,
  onPick,
}: ShiftSaveSideCardProps) {
  // Keyed by URL: a new picture gets a new chance, and an error that fires
  // before any effect runs is not reset by one.
  const [failedUrl, setFailedUrl] = useState<string | undefined>()
  const fail = () => setFailedUrl(imageUrl)
  const ended = formatSaveEndedAt(endedAt, now)
  const played = formatSavePlayed(durationSeconds)
  const showImage = imageUrl !== undefined && imageUrl !== "" && failedUrl !== imageUrl

  return (
    <button
      type="button"
      className="shift-save-side"
      data-save-side={actionId}
      data-focused={focused || undefined}
      aria-label={`${device}. ${ended}. ${played}.`}
      onFocus={onFocus}
      onClick={() => onPick(actionId)}
      {...shiftDesignPartAttrs(SHIFT_DESIGN_PARTS.saveSideCard, device)}
    >
      <span className="shift-save-side-shot">
        {showImage ? (
          <>
            {/* A blurred copy fills the space the whole screenshot leaves. */}
            <img className="shift-save-side-shot-fill" src={imageUrl} alt="" aria-hidden onError={fail} />
            <img src={imageUrl} alt="" loading="eager" onError={fail} />
          </>
        ) : (
          <span className="shift-save-side-noshot">
            <ImageOff aria-hidden />
            <span>No screenshot</span>
          </span>
        )}
      </span>
      <span className="shift-save-side-copy">
        <span className="shift-save-side-device" title={device}>{device}</span>
        <span className="shift-save-side-meta">
          <span>{ended}</span>
          <span>{played}</span>
        </span>
      </span>
    </button>
  )
}
