import { fixtureEncryptedSecret } from "../../fixtures/fixture-save-data"
import { ZonePartFrame } from "../../fixtures/ZonePartFrame"
import { ZoneSecretSlab } from "./ZoneSecretSlab"

export const name = "Secret slab"
export const note = "The backup's text on white: groups of four, thirty-two to a line, every line on screen"

/** A 162-character NIP-49 backup: six lines. */
export default function Default() {
  return <ZoneSecretSlab label="Backup secret" secret={fixtureEncryptedSecret} />
}

/** A short string: one line, centred. */
export function OneLine() {
  return <ZoneSecretSlab label="Backup secret" secret={fixtureEncryptedSecret.slice(0, 20)} />
}

/** A column: lines wrap whole groups and the slab scrolls. */
export function NarrowTall() {
  return (
    <ZonePartFrame shape="narrow-tall">
      <ZoneSecretSlab label="Backup secret" secret={fixtureEncryptedSecret} />
    </ZonePartFrame>
  )
}

/** A strip: the type is as large as the height lets every line be. */
export function WideShort() {
  return (
    <ZonePartFrame shape="wide-short">
      <ZoneSecretSlab label="Backup secret" secret={fixtureEncryptedSecret} />
    </ZonePartFrame>
  )
}
