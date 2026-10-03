/**
 * The back of the box: one tape, readable.
 *
 * Two call shapes. On the shelf it only shows the tape focus is on. In your
 * hand it adds what you can do with it: play it (asking where, when Korri
 * offers a real choice) or put it back. Put it back is a button as well as
 * Back, so a touch can do it.
 */
import "./BoxbusterCounterTape.css"
import type { CounterTape } from "./boxbuster-store-view"

export function BoxbusterCounterTape({ tape }: { tape: CounterTape }) {
  return (
    <article className="boxbuster-counter-tape">
      <BoxbusterCounterTapeFace tape={tape} />
    </article>
  )
}

export function BoxbusterCounterTapeInHand({
  tape,
  onPlay,
  onPutDown,
}: {
  tape: CounterTape
  onPlay: (locationId?: string) => void
  onPutDown: () => void
}) {
  return (
    <article className="boxbuster-counter-tape" data-in-hand="">
      <BoxbusterCounterTapeFace tape={tape} />
      <div className="boxbuster-counter-tape-actions">
        {tape.launch._tag === "Here" ? (
          <button
            type="button"
            className="boxbuster-counter-tape-play"
            data-focus-home=""
            onClick={() => onPlay()}
          >
            {tape.verb}
          </button>
        ) : (
          tape.launch.locations.map((location, i) => (
            <button
              key={location.id}
              type="button"
              className="boxbuster-counter-tape-play"
              {...(i === 0 ? { "data-focus-home": "" } : {})}
              onClick={() => onPlay(location.id)}
            >
              {`${tape.verb} on ${location.label}`}
            </button>
          ))
        )}
        <button
          type="button"
          className="boxbuster-counter-tape-put-down"
          onClick={onPutDown}
        >
          Put it back
        </button>
      </div>
    </article>
  )
}

function BoxbusterCounterTapeFace({ tape }: { tape: CounterTape }) {
  return (
    <>
      <div className="boxbuster-counter-tape-cover">
        {tape.coverArtUrl === undefined ? (
          // No art from Korri: the box shows its title on a plain sleeve.
          <span className="boxbuster-counter-tape-sleeve" aria-hidden="true">
            {tape.title}
          </span>
        ) : (
          <img src={tape.coverArtUrl} alt="" />
        )}
      </div>
      <div className="boxbuster-counter-tape-label">
        <h2 className="boxbuster-counter-tape-title">{tape.title}</h2>
        {tape.subtitle === undefined ? null : (
          <p className="boxbuster-counter-tape-subtitle">{tape.subtitle}</p>
        )}
        {tape.facts.length === 0 ? null : (
          <ul className="boxbuster-counter-tape-facts">
            {tape.facts.map(fact => (
              <li key={fact}>{fact}</li>
            ))}
          </ul>
        )}
      </div>
    </>
  )
}
