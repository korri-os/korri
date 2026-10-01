import "./PicoMenu.css"
import { useEffect, useId, useRef } from "react"
import { PicoMenuKey } from "../atoms/PicoMenuKey"
import { PicoRow } from "../atoms/PicoRow"
import { PicoCard } from "../molecules/PicoCard"

/** One line in the list: what it is called, its current value, what it does. */
export interface PicoMenuEntry {
  readonly label: string
  readonly detail?: string
  readonly onPress: () => void
}

/**
 * A key on the floor row, and the short list it opens above itself.
 *
 * The list is a blue card, the same object as the pause menu over a game. It
 * covers part of the screen and dims nothing, because PICO-8 cannot dim. While
 * it is open the d-pad stays inside it, so the cursor cannot wander onto the
 * screen behind. Opening puts the cursor on the first entry. Closing puts it
 * back on the key, unless something else already took it.
 *
 * Whether the list is open belongs to the owner, because Back closes it and
 * Back arrives through the host. `onAim` reports the name of the focused key
 * or entry, so the owner's hints can say what A does on it.
 */
export function PicoMenu({
  label,
  entries,
  open,
  onToggle,
  onAim,
  claimFocus = false,
  onFocusClaimed,
}: {
  readonly label: string
  readonly entries: readonly PicoMenuEntry[]
  readonly open: boolean
  readonly onToggle: () => void
  readonly onAim?: (label: string | undefined) => void
  /** Put the cursor on the key when this menu appears. */
  readonly claimFocus?: boolean
  readonly onFocusClaimed?: () => void
}) {
  const listId = useId()
  const key = useRef<HTMLButtonElement>(null)
  const list = useRef<HTMLUListElement>(null)

  useEffect(() => {
    if (!open) return
    const home = key.current
    const shown = list.current
    shown?.querySelector("button")?.focus()
    return () => {
      const active = document.activeElement
      const lost = active === null || active === document.body || !active.isConnected
        || shown?.contains(active) === true
      if (lost && home?.isConnected === true) home.focus()
    }
  }, [open])

  // biome-ignore lint/correctness/useExhaustiveDependencies: The claim is read when the menu appears.
  useEffect(() => {
    if (!claimFocus) return
    key.current?.focus()
    onFocusClaimed?.()
  }, [claimFocus])

  return (
    <div
      className="pico-menu"
      onBlur={(event) => {
        const next = event.relatedTarget
        if (!(next instanceof Node) || !event.currentTarget.contains(next)) onAim?.(undefined)
      }}
    >
      {open ? (
        <div aria-label={label} className="pico-menu-list" data-block-exit="true" id={listId} role="dialog">
          <PicoCard tone="tell">
            <ul className="pico-menu-entries" ref={list}>
              {entries.map((entry) => (
                <li key={entry.label} onFocus={() => onAim?.(entry.label)}>
                  <PicoRow detail={entry.detail} label={entry.label} onPress={entry.onPress} />
                </li>
              ))}
            </ul>
          </PicoCard>
        </div>
      ) : null}
      <PicoMenuKey
        controls={open ? listId : undefined}
        expanded={open}
        label={label}
        onFocus={() => onAim?.(label)}
        onPress={onToggle}
        ref={key}
      />
    </div>
  )
}
