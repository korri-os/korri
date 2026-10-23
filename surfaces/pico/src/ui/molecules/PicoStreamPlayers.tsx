import "./PicoStreamPlayers.css"

/**
 * Who is playing on this game's stream from another device, as the pause
 * menu says it: a quiet label, then each name on its own dark plate. Names
 * are Korri's (a person name, or a device label when the person has none);
 * a long one wraps, and many wrap onto more lines.
 *
 * A fact, not a control: nothing here takes focus.
 */
export function PicoStreamPlayers({ names }: { readonly names: readonly string[] }) {
  return (
    <section aria-label="On the stream" className="pico-stream-players">
      <h3 className="pico-stream-players-label">ON THE STREAM</h3>
      <ul className="pico-stream-players-names">
        {names.map((name, index) => (
          // Two devices may carry the same label; the position keeps them apart.
          <li className="pico-stream-players-name" key={`${index}:${name}`}>{name}</li>
        ))}
      </ul>
    </section>
  )
}
