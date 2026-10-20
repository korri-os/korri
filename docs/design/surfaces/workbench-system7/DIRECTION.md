# Workbench · System 7: Korri as the Finder, 1-bit chrome, hand-made colour icons

## Colour and type: 1-bit chrome, the 34 Apple Icon Colors for icons only
Chrome is two pens: black `#000000` and white `#FFFFFF`; desktop and scroll tracks are the 50%
checker (`0x55/0xAA`); inactive titles and dimmed buttons are that checker through text.
Icons use the ResEdit "Apple Icon Colors" set (13 greys, 15 tints, 6 rainbow) and nothing else:
`PAL` in index.html names each with one char; `DARK` steps every colour to its darker neighbour
for Finder selection (black stays). Off the icons: list selection cyan `#99FFFF` (the Open dialog)
and the Control Strip / Key Caps greys `#DDDDDD` / `#CCCCCC`. Type: both Giles Booth faces are
1024-upem with 64 units a pixel, so each is set at `font-size:16px` to land one glyph pixel on one
pixel: ChiKareGo2 as Chicago 12 (`16px/16px`, cap 9px; at 12px it scales 0.75 and antialiases) for
titles, menus, buttons, dialogs; FindersKeepers as Geneva 9 (`16px/12px`, x-height 5px; 9px gives
3px) for labels, header, Get Info. Geneva "bold" is the glyphs smeared 1px, as System 7 did.
Checked: title, menu bar, alert and label crops hold only `#000` and `#FFF`.

## Skeleton (one sentence)
A Games disk and a Control Panels folder on the grey desk open Finder windows under the menu bar with the clock; a game is a chassis icon whose Get Info window holds the cover and the places.

## Layout: `plan(width, height)` in index.html (thresholds kept from workbench/)
Zoom 2 at both sizes: 640×360 logical in overlap, 320×240 in stack. Overlap: Games front, Control Panels behind, desktop icons down the right edge (4px clear at the bottom), Control Strip bottom-left, Key Caps bottom-right.
Stack: the front window fills the width; every other window and the Trash are System 7.5 pop-up
windows, tabs along the bottom edge; the Control Strip shrinks to its tab; Label and Special (no
items behind them) yield to the clock. The Keyboard owns the bottom edge while open; the Join
dialog sits lower-right so the selected row stays in view; the Wi-Fi window keeps its height under
the Keyboard and gets its buttons back when it closes. Get Info never scrolls: its Comments box
moves beside the 1× cover when the width allows. State survives a resize.
## Memorable element
Hand-made 32×32 colour icons (floppy, bricks, carts, jewel case, dog-eared document) on a 1-bit desk: char-maps, 1px black outline, top-left light; selection darkens them and inverts the label.
## Text entry
A text field is the field; confirm on it opens a Keyboard window of Key Caps keys under it. The
d-pad moves between keys, confirm types at the insertion point (the key draws pressed); up from the
top row enters the field; back deletes, cancels on an empty field; Shift, Space, Cancel, Join end the last row.
## States (`?state=`; `?fixture=` still works)
idle (default). game: Get Info on Spelunky; the cover is Pico's cavern picture quantised to the 34
colours (2% luminance stretch, 4×4 Bayer at ±20), 2× in overlap, 1× in stack. wifi: the Wi-Fi
control panel, Join Kestrel over it, Key Caps under it; rows other than hummingbird-lake are
illustrative. problem: the Stop alert, Games inactive behind. `options` drops the File menu; `menu`
cycles the front window. Trash only writes the header line. Inactive bars go blank; Key Caps draws active.

## Capability map
C1  Loading: "Reading disk…" in the Games header line; Error: an alert "The disk “Games” could not be read", Retry/Cancel; Empty: the window text "This disk is empty. Games show here once Korri finds them…".
C2  Games window, icon view: a chassis icon per system, the title as its label; "(paused)" a second label line and the icon in the Finder's open (dotted) look; a section is a folder inside Games; the cover shows large only in Get Info.
C3  The Finder header's second line: "Spelunky  PC  Plays on This device, zao  Last played 9 days ago, 7 plays"; "Paused, resumes where you left it" for a resumable game; "No cover" when there is none; Get Info's Comments box reads "Two places. Choose one, then Play.", then "Starting on This device" on Play.
C4  Confirm on an icon with one place starts it ("Starting on This device" in the header line); two places send the person to Get Info, where "Plays on" is a radio button per place.
C5  Get Info: the cover where the icon sits, Kind / Last played / Plays / Plays on as bold Geneva labels, a Comments box, Play / Information… / Forget… along the foot; a dimmed button is the checker through its text; Forget… raises a Caution alert "Forget Spelunky? Its saved facts are removed." Cancel/Forget.
C6  Find: File > Find… puts a text field in a dialog over the Games window; icons whose title lacks the text go to the open (dotted) look as each letter is typed.
C7  Busy: an alert titled with the kicker, the detail as its body, the host's actions as its buttons (Cancel); the Games window behind goes inactive.
C8  Running: the game has the screen; the application menu at the right of the menu bar shows the game's icon and name, as it showed the front application.
C9  Problem: a Stop alert: "Could not start", "Spelunky: zao did not answer." then "Try again, or cancel.", Cancel / Retry with the default ring on Retry; the Games window behind goes inactive and its header's second line reads "Spelunky: could not start on zao".
C10 Special > Update Disk on the Games disk re-reads it, where Special held Eject and Erase Disk; the header shows "Reading disk…" while it runs.
C11 Get Info's Information… button opens the route sheet: the chosen route as "Kind", every route a line with its warnings, the saved one marked with the Finder's check.
C12 The Control Panels folder holds Device, Wi-Fi and Identity control-panel icons; each opens its own control panel window.
C13 Choice: a pop-up menu; text: a text field; secret: a field of bullets with a Clear button labelled clearLabel; action: a button; confirm: an alert with its title, message, confirm button.
C14 Saving: the control dims and the window title reads "Saving…"; Problem: a Note alert with the message and one Continue button.
C15 Buttons along the Device control panel's foot, one per device action, the last with the default ring.
C16 The Identity control panel's "Back Up…" button asks a password in a dialog; the result: the secret in a read-only field and a QR drawn in black on white.
C17 "Switch from Backup…": a dialog with two fields (secret, password), a Transfer/Delete pop-up menu, a trust-loss check box (an X when set), Cancel/Switch.
C18 "Switch by Bunker…": the same dialog with one scrolling field for the URI, the pop-up and the check box, Cancel/Switch.
C19 "Retired Keys": a list box with a 16px scroll bar, one key per row, cyan selection; Delete dims until the "Backed up" check box holds its X.
C20 Working: the window title reads the operation, its controls dim; Problem: a Note alert with the message and Continue; Switched: "Now signed as <key>" in the window.
C21 The Control Strip at the bottom-left: a module with four bars and "hummingbird-lake 72%"; "Wired" with a plug glyph; "Offline" with hollow bars; confirm opens the Wi-Fi control panel; in stack mode only its tab shows and stays a stop.
C22 The Wi-Fi control panel: a list box (black border, 16px scroll bar) strongest first with 4-bar glyphs, a padlock when secured, a check when known; NoAdapter "No Wi-Fi hardware"; scanning "Wi-Fi: Scanning…" in the title; attempt "Connecting to X" or "X: message" with a Retry Password button; a Scan button.
C23 Confirm on a listed network raises "Join <SSID>" in the 4px compound frame: "Password for <SSID>:", a field of bullets with "6/63", Cancel/Join, the Keyboard under it; Scan, Hidden Network… and Cancel along the foot; Hidden Network… asks name, then password.
C24 A window pulled over the game, titled with the overlay title: groups as ruled boxes with their titles (as General Controls drew them), a command a button, a toggle a check box with its two labels, a choice a pop-up menu, a range a slider showing its value, dimmed with reason; the close box dismisses.
C25 No answer yet
C26 The clock at the right of the menu bar before the application menu; the build label "korri 0.4.1 · 9a1fb22" in the Finder header line in place of "available".
C27 After a while the screen goes black with the Finder's zoom rectangles stopped, as After Dark's "Starry Night" did without the stars; any input brings it back.
C28 options drops the File menu; menu cycles the front window; system does nothing; back closes the front window or cancels the dialog; every box, button, icon, row, key and tab is a stop.
C29 A text field is the field; confirm opens a Keyboard window of Key Caps keys under it, the d-pad moves between keys, confirm types at the insertion point, up from the top row enters the field; back deletes, and closes the keyboard on an empty field; Shift, Space, Cancel and Join end the last row.
C30 Focus is the Finder's selection: the darkened icon with the inverted label, the 3px default ring on a button, a pressed close/zoom box or key, an inverted menu item, tab or radio label, a cyan list row; a dialog traps focus and inactivates the window behind; zoom rectangles dropped under reduced motion.
C31 Confirm: a Caution alert, the kicker as title, the detail as body, the game named first; buttons in the host's order and words, focus on the first
C32 The Busy alert gains the Finder's copy bar, filled black to done over total, "1.2 MB of 3.4 MB" under it; no total "1.2 MB read", no bar; no progress the body alone
C33 The header's second line adds "Needs a 3.4 MB download first" ("Needs a download first" with no size); Get Info adds a bold Download label with the size
