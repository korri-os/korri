import { ZoneGlyphSheet } from "../../fixtures/ZoneGlyphSheet"
import { ZoneCaps } from "./ZoneCaps"

export const name = "Caps"
export const note = "Zone Caps, Zone's own pixel capitals, in five inks at caps-1, caps-2 and caps-3, and Korri's facts in Jersey 10"

export default function Default() {
  return <ZoneCaps size="button" children="Play here" />
}

/** One word with no break point: it truncates with an ellipsis inside its box. */
export function LongWord() {
  return <ZoneCaps size="foot" children="Hummingbird-lake-east-wing-upstairs-office-extender-5ghz" />
}

export function Tab() {
  return <ZoneCaps size="tab" children="Save data" />
}

export function Hint() {
  return <ZoneCaps size="hint" children="Confirm" />
}

export function Button() {
  return <ZoneCaps size="button" children="Play on zao" />
}

export function Foot() {
  return <ZoneCaps size="foot" children="21:47" />
}

/** Korri's fact in its own case: Jersey 10, as tall as the caps around it. */
export function Fact() {
  return <ZoneCaps fact size="caps-1" children="hummingbird-lake · /nix/store/8n1x2-retroarch" />
}

export function Caps2() {
  return <ZoneCaps size="caps-2" children="Save data" />
}

export function Caps3() {
  return <ZoneCaps size="caps-3" children="Paused" />
}

export function Hud() {
  return <ZoneCaps ink="hud" size="caps-1" children="Time 21:47" />
}

export function Banner() {
  return <ZoneCaps ink="banner" size="caps-2" children="Games" />
}

export function Danger() {
  return <ZoneCaps ink="danger" size="caps-1" children="Delete" />
}

/* The glyph sheet: every glyph, at each size, in each ink. */
export function SheetWhite1() { return <ZoneGlyphSheet ink="white" size="caps-1" /> }
export function SheetWhite2() { return <ZoneGlyphSheet ink="white" size="caps-2" /> }
export function SheetWhite3() { return <ZoneGlyphSheet ink="white" size="caps-3" /> }
export function SheetHud1() { return <ZoneGlyphSheet ink="hud" size="caps-1" /> }
export function SheetHud2() { return <ZoneGlyphSheet ink="hud" size="caps-2" /> }
export function SheetHud3() { return <ZoneGlyphSheet ink="hud" size="caps-3" /> }
export function SheetBanner1() { return <ZoneGlyphSheet ink="banner" size="caps-1" /> }
export function SheetBanner2() { return <ZoneGlyphSheet ink="banner" size="caps-2" /> }
export function SheetBanner3() { return <ZoneGlyphSheet ink="banner" size="caps-3" /> }
export function SheetInk1() { return <ZoneGlyphSheet ink="ink" size="caps-1" /> }
export function SheetInk2() { return <ZoneGlyphSheet ink="ink" size="caps-2" /> }
export function SheetInk3() { return <ZoneGlyphSheet ink="ink" size="caps-3" /> }
export function SheetDanger1() { return <ZoneGlyphSheet ink="danger" size="caps-1" /> }
export function SheetDanger2() { return <ZoneGlyphSheet ink="danger" size="caps-2" /> }
export function SheetDanger3() { return <ZoneGlyphSheet ink="danger" size="caps-3" /> }
