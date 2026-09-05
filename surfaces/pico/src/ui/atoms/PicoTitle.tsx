/**
 * A heading in the display face.
 *
 * Size is a step on the type ramp, chosen by the caller, so the same heading
 * can lead a screen or label a card without either one restating a font-size.
 * Level is separate from size: a card's title may look small and still be the
 * heading a screen reader lands on. Text is a string, not children: a title
 * is one line the tool can edit, never a slot something else composes into.
 */
export function PicoTitle({
  text,
  size = "lg",
  level = 1,
  tone = "ink",
}: {
  readonly text: string
  readonly size?: "sm" | "md" | "lg" | "xl"
  readonly level?: 1 | 2 | 3
  /** A role the screen already has, so a heading never invents a colour. */
  readonly tone?: "ink" | "accent" | "warn"
}) {
  const Tag = `h${level}` as const
  return (
    <Tag className="pico-title" data-size={size} data-tone={tone}>
      {text}
    </Tag>
  )
}
