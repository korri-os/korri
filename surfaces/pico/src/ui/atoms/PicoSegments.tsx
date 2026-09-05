/**
 * A run of labelled segments with one lit.
 *
 * Legacy's two-segment ON/OFF toggle, generalised: Korri's choices are not
 * always two, and a control that could only draw two would be wrong the first
 * time a plugin offered three. Purely presentational — the row it sits in
 * decides what pressing does.
 */
export function PicoSegments({
  options,
  current,
}: {
  readonly options: readonly string[]
  readonly current: number
}) {
  return (
    <span className="pico-segments">
      {options.map((option, index) => (
        <span
          className="pico-segments-item"
          data-lit={index === current ? "true" : undefined}
          key={option}
        >
          {option.toUpperCase()}
        </span>
      ))}
    </span>
  )
}
