# Catalogue

**Skeleton:** the library is a mail-order catalogue: a spread of two facing pages (Games A–M, Games N–Z), thumb tabs down the fore-edge to the Index at the front and the Order form at the back, and every non-browsing capability (device, Wi-Fi, account, launching) lives on the order form.

## Colours (two-colour job on grey newsprint)

| Name | Hex | Use |
|---|---|---|
| Newsprint | `#d3d4cd` | the paper; photos multiply into it |
| Press ink | `#1d2136` | all type, rules, plate edges, the Order form tab |
| Process magenta | `#b00c62` | the one spot colour: item numbers, current tab, "You left off here", How to order box |
| Graphite | `#36373d` | the pencil mark only; never printed |
| Page back | `#c3c4bc` | the dog-ear, page curls, unselected tabs |
| Desk | `#474c55` | the table under the book |

## Type

- **Geist** (local variable font, embedded as woff2). A plain grotesque at 500–900. It has no serif and no broadsheet manners. Item numbers are set at 900 with tight tracking in magenta. They are the typographic moment. Play facts sit where a price would go: the count in bold and "last …" in small print.
- **m5x7** (CC0, from Pico's folder) is used only *inside* the four box-art panels I drew. Never for catalogue copy.

## The memorable element

A **graphite pencil loop**. It is drawn twice round, a little off, around whatever has focus. It moves between items and onto the fore-edge tabs. The rest of the page stays quiet print.

## How the skeleton reads from this spread

- Running header: `Order line hummingbird-lake, 72% clear` / `The mini edition. Play facts correct at 21:47`. Season and order line, in the paper's idiom.
- Fore-edge tabs: Index (front), Games A–M, Games N–Z (magenta, here), and **Order form**, printed in solid ink with the words "device, Wi-Fi, account". The pencil reaches the tabs by moving right off the page. A is the only button needed.
- Two places print as empty tick boxes ("choose one on the order form"). Celeste has the dog-eared page corner and "You left off here". Comet Courier gets the crossed "No picture" panel.
- At 640×480 one page shows. The facing page is a labelled sliver at the gutter ("one turn back"). Moving left off the page turns it, and the same item stays circled.

## Text entry: the order form's box-per-letter fields

The Wi-Fi password field is printed like a paper form: **one box per character**. Under it is a printed **letter chart** of four rows (a–z, A–Z, 0–9, symbols). The d-pad moves the pencil on the chart. **A** writes that character into the next empty box. **B** rubs out the last box. A printed "show letters" tick box decides if the boxes show the characters or dots. The pencil goes from the chart to the "Join" line by moving down off the chart.

**QR code:** it is printed in the top corner of the order form as an **"Order by phone" panel**, beside the Wi-Fi and Account sections. It is the fast route for the 40-character identity string and for passwords after the device is online. Scanning it fills the same boxes. The letter chart stays the fallback for a first Wi-Fi join, because the phone cannot reach the device until it has a network. (Assumption: Korri would need to publish such a phone hand-off. Today it does not.)

- The sample art carries baked-in titles for 7 games. Petal Quest, Super Metroid, Zelda and Wario Land 4 have no matching picture, so I drew those four as SVG box art at their own ratios. Comet Courier's picture is not used, because the brief says it has no cover.
- The stress set in this one screen: a long title (Zelda wraps to 3 lines), a missing cover, zero plays next to 112, and two-place items. Items narrower than 150px move the play facts under the title. Blank, loading and error states are not drawn (one screen only).
