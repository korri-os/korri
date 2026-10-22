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

## Capability map
C1  Loading: empty plates and "still at the printer" in the running head; Error: an erratum slip across the spread with the message; Empty: one page, "Nothing listed this edition".
C2  Games pages, A to Z: title, system first in the small print, a plate at the picture's ratio, no picture the crossed panel; a section its own page.
C3  Play facts where the price goes: the count bold, the time under it as the unit line, total time a third line; a resumable game has a folded corner.
C4  Write the item number on the Order form, the dark tab at the back; a two-place item prints empty tick boxes, and the form's "Play on" line must be ticked.
C5  The Order form's "Options for item 04" block: a line per action with small print; a disabled line prints "not this edition"; destructive needs "I am sure".
C6  The Index tab at the front: a printed letter chart; letters ticked with the pencil narrow the title list below it, each title with its item number and page.
C7  Busy: a despatch slip laid across the spread with the kicker as its heading, the detail under it, and the host's actions as tick boxes (Cancel order).
C8  Running: a DESPATCHED rubber stamp over the item's plate with the kicker in its ring; the rest of the spread stays print.
C9  Problem: a returns slip over the spread: kicker as heading, the game's title on the "Item" line, reason beneath; boxes "Try again" (when it can), "Discard".
C10 The "Send me a new catalogue" tick box at the foot of the Order form, printed again on the erratum slip.
C11 The form's "How it will play" block: a route per line (runnerBuild, program), magenta warnings under it, the saved route "as last time"; state as heading.
C12 The Order form's "This device" section: each group a ruled box with its title, each item a form line: label, value in the box, description in small print.
C13 Choice: a row of tick boxes; text: box-per-letter; secret: dots with "show letters" and "clear" ticks; action: a tick box that fires; confirm: a printed notice with its own box.
C14 Saving: the value stays pencil until it prints in ink; Problem: a magenta correction note under the line with the message, tick "Noted" to clear it.
C15 Tick boxes at the foot of the "This device" section (Restart, and the rest), one per device action.
C16 The Account section's "Request a copy of your account": password in the boxes; the reply prints as a cut-out voucher, the secret in letters and as a QR.
C17 The Account section's "Move my account here": boxes for secret and password, tick Transfer or Delete, tick the trust-loss line, then Send.
C18 No answer yet
C19 The Account section's "Previous account numbers": each retired key a line with a "Remove" box that only prints once "I have my copy" is ticked.
C20 Working: a slip over the Account section with the operation; Problem: a magenta correction note with the message; Switched: "Your new account number" line.
C21 The running head's order line: "Order line hummingbird-lake, 72% clear" with the bars; "Order line: wired"; "Order line closed" when offline.
C22 The Wi-Fi section: exchanges strongest first with bars, a padlock when secured, "on your account" when known; NoAdapter "No line at this address"; scanning "Checking lines"; attempt "Connecting to X" or "X refused: message", "Try the password again"; "Look again" scans.
C23 Tick a listed exchange and fill its password boxes, then the "Join" line; the last entry "Other exchange" has boxes for its name and its password.
C24 The loose insert card over the game: title as heading, groups as ruled blocks, a command a tick box, a toggle two boxes (trueLabel, falseLabel), a choice a box row, a range a printed scale; B puts it back.
C25 The "New customer" page at the front: a line per controller that ticks itself when heard, then Wi-Fi, device name and account lines, and "Open my account".
C26 The clock in the running head ("Play facts correct at 21:47"); the build label in the imprint ("korri 0.4.1 · 9a1fb22, Retroid Pocket Mini").
C27 Nothing: the book lies open where it was left, the pencil loop still round the last item.
C28 options jumps the pencil to the Order form tab, menu to the Index tab; system does nothing; the d-pad reaches the tabs right off the page; B turns back.
C29 Box-per-letter fields with the printed four-row letter chart: the d-pad moves the pencil, A writes the next box, B rubs out the last, a "show letters" tick decides dots or letters, down off the chart reaches Join; the "Order by phone" QR is the fast route once Korri publishes a hand-off.
C30 The graphite pencil loop is always round one item or tab and redraws itself in 0.42 s when it moves; under reduced motion it appears in place, no draw-on.
C31 Confirm: an advice note laid across the spread with the kicker as its heading, the game's title on the "Item" line when it has one, the detail under it; a tick box per host action, in the host's order and words, none ticked in advance.
C32 Progress: the despatch slip prints a scale under its heading, inked to done over total, "1.2 of 3.4 GB" beside it; with no total "1.2 GB so far" and no scale; with no progress the slip alone.
C33 The item's small print after the system: "Needs a 3.4 GB download first"; with no size, "Needs a download first".
C34 No answer yet
C35 No answer yet
C36 No answer yet
C37 No answer yet
C38 No answer yet
C39 No answer yet
C40 No answer yet
C41 No answer yet
