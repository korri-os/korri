# Catalogue

**Skeleton:** the library is a mail-order catalogue: a spread of two facing pages (Games A–M, Games N–Z), thumb tabs down the fore-edge to the Index at the front and the Order form at the back, and every non-browsing capability (device, Wi-Fi, account, launching) lives on the order form.

## Colours (two-colour job on grey newsprint)

| Name | Hex | Use |
|---|---|---|
| Newsprint | `#d3d4cd` | the paper, with two layers of ink-coloured noise: fine fibre and a slow mottle |
| Press ink | `#1d2136` | all type, rules, keylines, the halftone screen, the Order form tab |
| Process magenta | `#b00c62` | the one spot colour: item numbers, current tab, "Still open", the How-to-order screen tint |
| Graphite | `#36373d` | the pencil mark only; never printed |
| Page back | `#c3c4bc` | the folded corner, page curls, unselected tabs |
| Desk | `#474c55` | the table under the book |

## Type

- **Geist** (local variable font, embedded as woff2), the only face. Item numbers at 900 with tight tracking in magenta are the typographic moment; a hair of text-shadow in the same ink stands for a solid spot pressed on soft stock. Titles 750, the system in 650, small print in soft ink. Play facts sit where a price goes: the count bold as the price, the time under it as the unit line ("41 plays / 2 hours ago"). Only an item narrower than 104px gives up that position and runs the facts under the title.
- The four games with no sample picture (Petal Quest, Super Metroid, Zelda, Wario Land 4) carry flat vector box art drawn at their systems' box ratios, with titles in Geist. No pixel font anywhere.

## Print

Every picture is a **plate**: a 1px keyline, a 3px paper margin, the picture multiplied into the paper, a soft inner edge where ink meets margin, and a 45° halftone screen over it. The running head sits on a thick-and-thin rule. The gutter darkens into the spine with a little show-through. The How-to-order box is a screen tint of the spot colour, not a flat fill.

## The memorable element

A **graphite pencil loop**, drawn twice round, a little off, around whatever has focus. It reaches 9px past the item; the column gap is wider than that, so it never touches a neighbour. It moves between items and onto the fore-edge tabs. The rest of the page stays quiet print.

## Order on the page

Titles run **A to Z** across each page, left to right then down, ignoring a leading "The" (Zelda files under L). Item numbers are Korri's catalog order, the number you write on the form, and so are not sequential on the page; the left page's foot says so. The featured item is larger but keeps its place in the run.

## How the skeleton reads from this spread

- Running header: `Order line hummingbird-lake, 72% clear` / `The mini edition. Play facts correct at 21:47`. The clock is one step above the small print, not a status bar.
- Fore-edge tabs: Index, Games A–M, Games N–Z (magenta, the open page), and **Order form** in solid ink with "device, Wi-Fi, account". The pencil reaches the tabs by moving right off the page; the magenta tab follows the page that is open.
- Two places print as empty tick boxes ("choose one on the order form"). Celeste's plate has a **folded-down corner** and reads "Still open. Carry on where you stopped." Comet Courier gets the crossed "No picture" panel.
- At 640×480 one page shows; the facing page is a labelled sliver at the gutter ("one turn back"). Moving left off the page turns it and lands on the item nearest the gutter. Below 520px of height the tabs drop their second line.

## Text entry: the order form's box-per-letter fields

The Wi-Fi password field is printed like a paper form: **one box per character**. Under it is a printed **letter chart** of four rows (a–z, A–Z, 0–9, symbols). The d-pad moves the pencil on the chart. **A** writes that character into the next empty box. **B** rubs out the last box. A printed "show letters" tick box decides if the boxes show the characters or dots. The pencil goes from the chart to the "Join" line by moving down off the chart.

**QR code:** printed in the top corner of the order form as an **"Order by phone" panel**, beside the Wi-Fi and Account sections. It is the fast route for the 40-character identity string and for passwords after the device is online; scanning it fills the same boxes. The letter chart stays the fallback for a first Wi-Fi join. (Assumption: Korri would need to publish such a phone hand-off. Today it does not.)
