import type { ReactNode } from "react"
import "./PicoPartFrame.css"

/** Supply the container contract that the shelf normally gives art and carts. */
export function PicoPartFrame({ children }: { readonly children: ReactNode }) {
  return <div className="pico-part-frame">{children}</div>
}
