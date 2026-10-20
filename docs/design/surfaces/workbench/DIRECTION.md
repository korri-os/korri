# Workbench: Korri as Workbench 2.x, documents in windows

## Colour and type: the four Workbench 2.x pens, one face
Workbench grey `#AAAAAA` (pen 0: desktop, window faces), detail black `#000000` (pen 1:
outlines, text, bevel shadow), shine white `#FFFFFF` (pen 2: screen bar, bevel light,
requester face), window blue `#6688BB` (pen 3: active title bar, selected icon). Bevels:
1px white over 1px black, reversed when sunk; no blur. Ghosting: pen 1 text under a 50%
checker of the face pen. Spleen 5x8 (BSD 2-Clause), inlined, at 8px: one glyph pixel is
one screen pixel, Topaz 8's cell; the screen zooms by a whole number. Pico's Spleen build
draws `·` empty, so the build label paints its dot.

## Skeleton (one sentence)
Drawer icons (Games, Prefs) on a grey desktop open into gadgeted windows, under a screen
bar that holds the build label, a Wi-Fi tool and the clock.

## Layout: `plan(width, height)` in index.html
Scale is the largest whole number that keeps a 320x240 screen, one step less if that buys
room (560x340) to overlap. Overlap: windows overlap; the Keyboard sits at the bottom edge,
right of centre. Stack: the front window is full width, others dock as title bars above the
Keyboard, which is full width at the bottom. A requester keeps its size (min 60% width in
stack), centred in the room above the Keyboard, over everything; focus is trapped in it and
back is Cancel. A focused gadget is sunk with a 1px pen-1 inner rim. State survives a resize.

## Memorable element
Workbench's complement selection (pen n to pen 3-n) turns the Spelunky chassis blue and white in
a sunk frame. Icons are chassis only; the cover shows large only in the game's own window.

## Text entry
A string gadget is the field. Confirm on it opens a Keyboard window of key gadgets under it;
the d-pad moves between keys, confirm types at the cursor. Up from the top key row enters the
gadget; down from the gadget goes to the requester's buttons, then the keys. Back deletes before
the cursor, cancels on an empty gadget. Shift, Space, Cancel and Use end the last row; Use sends.

## States (`?state=`; `?fixture=` still works)
idle (default). game: confirm on Spelunky opens its window; its cover is Pico's cavern picture
quantised to the four pens (2% luminance stretch, 4x4 Bayer at ±48), 2x on a screen 300px tall,
else 1x; it read well (title, torches, door, gold), so no hand-drawn cover. wifi: Prefs > Wi-Fi,
confirm on Kestrel raises Join over the Keyboard (focus on `k`; letters type); rows other than
hummingbird-lake are illustrative. problem: Spelunky on zao failed; Games inactive under Retry/Cancel.

## Capability map
C1  Loading: "Reading drawer…" in the Games window's Info line; Error: a requester "Games drawer could not be read", Retry/Cancel; Empty: the window text "This drawer is empty. Games show here once Korri finds them…".
C2  Games window: a chassis icon per system, the title as label, "(paused)" a second line; a section a drawer inside Games; the cover large only in its window.
C3  The Info line at the window foot: "Last played 9 days ago, 7 plays", total time after it, "Paused, resumes where you left it" for a resumable game; the game's own window reads "Spelunky: 2 places", then "Starting on This device" on confirm.
C4  Confirm on an icon with one place starts it ("Starting on This device" in the Info line); two places raise a requester "Play Spelunky on", a button each.
C5  The game's own window: the cover large in a sunk frame, the facts as lines, "Play on" with a button per place, Play / Information… / Forget… along the foot; disabled ghosted with a checker; Forget… raises "Forget Spelunky?" with Forget/Cancel.
C6  A Find requester on the Games window with one string gadget; icons whose title lacks the text ghost as each letter is typed.
C7  Busy: a requester titled with the kicker, the detail as its body, the host's actions as its buttons (Cancel); the Games window behind goes inactive.
C8  Running: the game has its own screen in front; the Workbench screen behind carries the kicker and the title in its screen bar.
C9  Problem: a Retry/Cancel requester as Workbench raised them: "Could not start" as title, "Spelunky: zao did not answer." then "Try again, or cancel." as body, Retry focused; the Games window behind goes inactive and its Info line reads "Spelunky: could not start on zao".
C10 An Update gadget beside the size gadget on the Games window's foot: re-reads the drawer, as Window > Update did.
C11 The game's Information window: Default Tool is the chosen route; routes as Tool Types (runnerBuild, program) with warnings; the saved one is Default Tool.
C12 The Prefs icons Device, Wi-Fi, Identity each open a Prefs editor: each group a ruled box with its title, each item a labelled gadget with its value.
C13 Choice: a cycle gadget; text: a string gadget; secret: a string gadget of dots with a Clear button labelled clearLabel; action: a button gadget; confirm: a requester with its title, message, confirm button.
C14 Saving: the gadget ghosts and the editor's title bar reads "Saving…"; Problem: a requester with the message and one Continue button.
C15 Button gadgets along the Device editor's foot, where Save/Use/Cancel sit in a Prefs editor, one per device action.
C16 The Identity editor's "Backup…" button asks a password in a string gadget; the result: the secret in a read-only string gadget and a QR in pen 1 on pen 2.
C17 The Identity editor's "Switch from backup…": two string gadgets (secret, password), a Transfer/Delete cycle gadget, a trust-loss check box, Use/Cancel.
C18 The Identity editor's "Switch by bunker…": the same window with one scrolling string gadget for the URI, the cycle gadget and the check box, Use/Cancel.
C19 The Identity editor's "Retired keys" list gadget with a scroll bar, one key per row; the Delete button ghosts until the "Backed up" check box is ticked.
C20 Working: the editor's title bar reads the operation, its gadgets ghost; Problem: a requester with the message and Continue; Switched: "Now signed as <key>".
C21 The Wi-Fi tool on the screen bar: bars and "hummingbird-lake 72%"; "Wired" with a plug glyph; "Offline" with empty bars; confirm opens the Wi-Fi editor.
C22 The Wi-Fi editor: a list gadget (sunk, scroll bar) strongest first with bars (4 cells), a key glyph when secured, a check mark when known; NoAdapter "No Wi-Fi hardware"; scanning "Scanning…" in the title bar; attempt "Connecting to X" or "X: message" with a Retry password button; a Scan button.
C23 Confirm on a listed network raises "Join <SSID>": "Password for <SSID>:", a string gadget of dots with "6/63", Join/Cancel, the Keyboard under it; Scan, Hidden network… and Cancel along the foot; Hidden network… asks SSID, then password.
C24 A Workbench window pulled up over the game's screen, titled with the overlay title: groups as ruled boxes, a command a button, a toggle a check box with its two labels, a choice a cycle gadget, a range a slider showing its value, ghosted with reason; close dismisses.
C25 No answer yet
C26 The clock at the right end of the screen bar; the build label "korri 0.4.1 · 9a1fb22" after the screen name on the same bar.
C27 The Blanker: after a while the screen goes to pen 1 black, as the Workbench 2.x Blanker commodity did; any input brings it back.
C28 options opens or raises Prefs; menu cycles the front window; system does nothing; back closes the front window; every gadget is a stop.
C29 A string gadget is the field; confirm opens a Keyboard window of key gadgets under it, the d-pad moves between keys, confirm types at the cursor, up from the top row enters the gadget; back deletes, and closes the keyboard on an empty gadget; Shift, Space, Cancel and Use end the last row.
C30 Focus is selection: the complemented icon in a sunk frame, label inverted, a sunk gadget with a pen-1 inner rim, a selected list row in pen 3; a requester traps focus and inactivates the window behind; zoom-box outlines dropped under reduced motion.
C31 Confirm: a requester titled with the kicker, the detail as its body, the game named first when it has one; a button per host action, left to right in the host's order and words, focus on the first; the Games window behind goes inactive.
C32 Progress: the Busy requester gains a fuel gauge, the sunk bar of the volume windows, filled in pen 3 to done over total, "1.2 MB of 3.4 MB" under it; with no total "1.2 MB read" and no gauge; with no progress the body alone.
C33 The Info line adds "Needs a 3.4 MB download first" after the play facts, "Needs a download first" with no size; the game's own window lists it with the facts.
