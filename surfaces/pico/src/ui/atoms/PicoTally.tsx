/**
 * Where you are in the shelf, as a count.
 *
 * A row of dots only reads while the library is small; at two hundred games it
 * becomes noise, and a surface that degrades at real library sizes is not
 * finished. A tally states the same fact at any size.
 *
 * The glyphs are split for the eye but named as one phrase for the ear: read
 * literally, "3 / 48" is announced as a fraction, which is not what it means.
 */
export function PicoTally({
  position,
  total,
}: {
  readonly position: number
  readonly total: number
}) {
  return (
    <span
      aria-label={`${position} of ${total}`}
      className="pico-tally"
      role="img"
    >
      <span aria-hidden>
        {position}
      </span>
      <span aria-hidden className="pico-tally-separator">
        /
      </span>
      <span aria-hidden>
        {total}
      </span>
    </span>
  )
}
