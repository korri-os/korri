import "../../pico-motion.css"
import "./PicoScopeTabs.css"

/**
 * Which runners a layer's values apply to: any runner, a family, one runner.
 * A row of tabs at the head of the layer's card; the chosen one is a dark
 * plate like the rows under it, so the rows read as its contents.
 *
 * Tabs, not a menu: choosing one changes which rows the card shows and
 * nothing else. When the row is short of room the tabs wrap onto a second
 * line rather than leave a runner out.
 */
export function PicoScopeTabs({
  scopes,
  current,
  label,
  onSelect,
}: {
  readonly scopes: readonly { readonly label: string; readonly kind: "any" | "family" | "runner" }[]
  readonly current: number
  /** What the tabs choose, for a screen reader: "Runner". */
  readonly label: string
  readonly onSelect: (index: number) => void
}) {
  return (
    <div aria-label={label} aria-orientation="horizontal" className="pico-scope-tabs" role="tablist">
      {scopes.map((scope, index) => (
        <button
          aria-selected={index === current}
          className="pico-scope-tabs-tab"
          data-kind={scope.kind}
          // A layer's scopes keep their order; their place is their identity.
          key={`${index}:${scope.label}`}
          onClick={() => onSelect(index)}
          role="tab"
          type="button"
        >
          {scope.label.toUpperCase()}
        </button>
      ))}
    </div>
  )
}
