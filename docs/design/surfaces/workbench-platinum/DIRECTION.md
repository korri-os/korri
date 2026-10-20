# Workbench, Platinum: Korri as a Mac OS 8/9 Finder

## Colour and type: the Platinum greys, one lavender
Face `#DDDDDD`, highlight `#FFFFFF`, shadow `#777777`, trough `#AAAAAA`, border `#000000`; lavender
`#CCCCFF` is the only accent (selection, focus rings); a picked menu item is white on `#333399`.
Desktop: an 8x8 woven tile in `#666699`, `#7777AA`, `#5C5C8F`, drawn in CSS. Every bevel is lit from
the top-left: 1px white top/left, 1px `#777777` bottom/right, 1px black outside. Charcoal has no libre
clone, so the system font is Chicago (Mac OS 8's Appearance panel offers it beside Charcoal):
ChiKareGo2 at `16px/15px` (its 16px cell gives cap 9, x-height 7; bold is the Mac smear,
`text-shadow: 1px 0`); Geneva 9 is FindersKeepers at `16px/12px` (x-height 5, cap 7). Both base64, no smoothing.

## Skeleton (one sentence)
A hard disk "Games", a folder "Control Panels" and the Trash at the desktop's right edge open into
striped-title Finder windows under a 20px menu bar (Apple, File, Wi-Fi tool, clock, Korri menu).

## Layout: `plan(width, height)` in index.html
Scale is the largest whole number that keeps 320x240, one step less if that buys 560x340 to overlap.
Overlap: Games (4x64 grid) centred, Control Panels partly behind, the Keyboard windoid centred at the
bottom. Stack: the front window fills the width under the menu bar; the others collapse WindowShade-
style to 21px bars at the bottom edge; confirm on a bar brings it to front. A dialog keeps its size
(min 80% width in stack), centred above the Keyboard. State survives a resize. Icons: 32x32 in the 8.5
32-bit style: boxes in three-quarter view (top face lit at its front-left, right side darkest), faces
shaded in true colour, outlines in shades not black, a soft two-step shadow cast from each silhouette.

## Memorable element
The 19px striped title bar: six etched lines that stop around the centred name and vanish the moment the window goes behind.

## Text entry
A sunk field with a 2px lavender ring is the string. Confirm on it opens the Keyboard windoid under
it; the d-pad moves between bevelled keys, confirm types at the caret, the focused key draws pressed.
Up from the top row enters the field; back in the field deletes before the caret; Shift, Space,
Cancel and Use end the last row; Use sends. With a d-pad a 40-character identity string is 40 confirms.

## States (`?state=`; `?fixture=empty|one|many|long`)
idle: Games front, Spelunky selected, the header's second line. game: "Spelunky Info", cover 2x in
overlap and 1x in stack, with its own pixels: Mac OS 8.5+ ran in millions of colours, so no dither
(a 216-colour dither was tried first and turned the brick muddy). wifi: the
sunk list (rows below hummingbird-lake are illustrative; the screen does not say so), "Join Kestrel" right
of centre over it (right-aligned, 60% wide in stack, where the bars behind step aside), the Keyboard below. problem: a Stop alert, Retry ringed; Games inactive, "Spelunky: could not start on zao."
## Capability map
C1  Loading: "Reading…" as the Games header's second line; Error: an alert "Games could not be read", Retry/Cancel; Empty: window text "This window is empty. Games show here once Korri finds them…".
C2  Games window: a chassis icon per system on the 64px grid, the title as label, "(paused)" a second label line at 50% dim; a section a folder inside Games; the cover large only in Get Info.
C3  The Finder header's second line: "Spelunky, PC. Plays on This device, zao. Last played 9 days ago, 7 plays."; Get Info's foot line "Spelunky, 2 places.", then "Spelunky: starting on This device." on Play.
C4  Confirm on an icon with one place starts it (the header line says so); two places raise a dialog "Play Spelunky on", a button each.
C5  Get Info: the cover in a sunk frame, Kind / Last played / Plays as bold labels, Plays on as radios, Comments in a sunk box, Play (default ring) / Information… (dimmed #777777) / Forget… along the foot; Forget… raises a Caution alert "Forget Spelunky?" Forget/Cancel.
C6  Find: a sunk field in the Games header; icons whose title lacks the text dim to 50% as each letter is typed.
C7  Busy: a movable modal dialog titled with the kicker, the detail as body, the host's actions as buttons (Cancel); Games goes inactive behind it.
C8  Running: the game owns the screen; the Finder behind carries the kicker and the title in its application menu.
C9  Problem: a Stop alert: "Could not start" bold, "Spelunky: zao did not answer." then "Try again, or cancel.", Cancel then Retry (ringed, focused); Games inactive, header line "Spelunky: could not start on zao."
C10 View > Update in the menu bar (and the File menu's last item when options drops it): re-reads the Games disk.
C11 Get Info's "Plays on" radios are the routes; the saved one is on; Information… (dimmed until the host has it) shows Tool-Types-like route detail with warnings.
C12 The Control Panels folder holds Device, Wi-Fi, Identity; each opens a control panel window: each group a bevelled box with an etched title, each item a labelled control.
C13 Choice: a pop-up menu button; text: a sunk field; secret: a sunk field of bullets with a Clear button labelled clearLabel; action: a push button; confirm: an alert with its title, message, confirm button.
C14 Saving: the control dims and the panel's title reads "Saving…"; Problem: a Stop alert with the message and one Continue button.
C15 Push buttons along the Device panel's foot, one per device action, the primary one ringed.
C16 Identity's "Backup…" asks a password in a sunk field; the result: the secret in a read-only field and a QR in black on white in a sunk frame.
C17 Identity's "Switch from backup…": two fields (secret, password), a Transfer/Delete pop-up, a trust-loss checkbox, OK/Cancel.
C18 Identity's "Switch by bunker…": the same panel with one scrolling field for the URI, the pop-up and the checkbox, OK/Cancel.
C19 Identity's "Retired keys": a sunk list with a 16px scroll bar, one key per row; Delete dims until the "Backed up" checkbox is ticked.
C20 Working: the panel's title reads the operation and its controls dim; Problem: a Stop alert with Continue; Switched: "Now signed as <key>" in the header line.
C21 The Wi-Fi tool on the menu bar: four signal bars; "Wired" a plug glyph; "Offline" empty bars; confirm opens the Wi-Fi panel.
C22 The Wi-Fi panel: a sunk list strongest first with 4-bar glyph, a padlock when secured, a check when known, the percentage at the right; NoAdapter "No Wi-Fi hardware"; scanning "Scanning…" in the foot line; attempt "X: message" with a Retry password button; a Scan button.
C23 Confirm on a listed network raises "Join <SSID>": "Password for <SSID>:", a sunk field of bullets with "6/63" and the lavender ring, Cancel then Join (ringed), the Keyboard windoid under it; Scan, Hidden Network…, Cancel along the foot; Hidden Network… asks the name, then the password.
C24 A Platinum window pulled over the game's screen, titled with the overlay title: groups as bevelled boxes, a command a push button, a toggle a checkbox with its two labels, a choice a pop-up menu, a range a slider with its value, dimmed with reason; close dismisses.
C25 No answer yet
C26 The clock at the right of the menu bar before the application menu; the build label "12 items, korri 0.4.1 · 9a1fb22" as the Games window's Finder header.
C27 The Blanker: after a while the screen goes black, as Energy Saver did; any input brings it back.
C28 options drops the File menu (Play, Get Info, Forget…, Control Panels…); menu cycles the front window, as the application menu does; system does nothing; back closes the front window; the Trash is a stop that does nothing and the header line says so.
C29 A sunk field is the string; confirm opens the Keyboard windoid under it, the d-pad moves between keys, confirm types at the caret, up from the top row enters the field; back deletes, and closes the keyboard on an empty field; Shift, Space, Cancel and Use end the last row.
C30 Focus is selection: an icon darkened 50% with its label on lavender, a push button with the 3px default ring 3px out, a field or radio with a 2px #6666CC ring (the black default ring stays on Play at the same time), a key drawn pressed, a list row on lavender; an alert traps focus and inactivates the window behind (stripes gone, boxes hidden, title #777777); zoom rectangles dropped under reduced motion.
C31 Confirm: a Caution alert, the kicker bold, the detail under it, the game named first; push buttons in the host's order and words, focus on the first
C32 The Busy dialog gains a sunk bar filled lavender to done over total, "1.2 MB of 3.4 MB" under it; no total "1.2 MB read", no bar; no progress the body alone
C33 The header's second line adds "Needs a 3.4 MB download first." ("Needs a download first." with no size); Get Info adds a bold Download label with the size
