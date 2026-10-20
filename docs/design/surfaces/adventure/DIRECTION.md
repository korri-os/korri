# Adventure room
A LucasArts SCUMM screen in the Monkey Island 2 register. One 320x200 canvas,
drawn pixel by pixel, scaled by the largest whole number that fits (x2 at
640x480, x3 at 1280x720) with a black letterbox. Scene in rows 0-127,
sentence line at row 130, verbs and inventory in rows 141-199.

## Colours (a hand-picked 58-entry palette; the covers are forced into it)
- Den plum `#291C3E`: the wall at 21:47, with `#352650` stripes.
- Walnut `#62391C`: bookcase, desk, TV cabinet.
- Screen glow `#9CE6FF`: the one loud colour. Only valid targets get it.
- Verb violet `#5F56D0` and lit verb `#CBC3FF`: the verb grid.
- Sentence teal `#62D6C4`: the sentence line. Panel black `#000000`.

## Type
Kenney Mini (Pico's `kenney-mini.glyphs.txt`, CC0), drawn from the glyph
table into the framebuffer at 1x, so it scales with the pixels. Its 7-row
cell is the size of SCUMM's own font, and its lowercase has a real
x-height, which a sentence line needs. All text uses it.

## Skeleton
Verb x object: the d-pad moves a crosshair between hotspots, the sentence
line names the hotspot, and confirm applies the verb (Use on a game box,
Look at on anything else, so the common case is one press). Up off the top
of the scene or down off the floor enters the verb grid; right from the
last verb column enters the inventory. Settings is the desk drawer, Wi-Fi
is the phone, Identity is the wallet, the clock is on the wall, this device
is the console under the TV, zao is the desk monitor. Look at prints facts.

## The one memorable element
"Use Spelunky with" in the sentence line while the TV and zao's monitor
both light up and show Spelunky. The two places are two lit objects.

## Text entry
Use the typewriter on the desk. The verb grid gives way to a keyboard grid
in the same 320-wide panel: four rows of letters, then Shift, Space, Rubout
and Done, moved with the d-pad and struck with confirm. The sentence line
becomes the typed line and the paper in the roller fills with it. A
40-character identity string uses the same grid and scrolls left as it
grows; B rubs out one character, Done puts the verbs back.

## Capability map
C1  Loading: empty shelves and "Unpacking…" on the sentence line, boxes appearing as they arrive; Error: the message printed over the scene, bookcase dark; Empty: empty shelves, Look at says "Nothing on the shelves yet."
C2  Boxes on the bookcase, sized by system; a section a shelf with its name on the edge; the cover forced into the palette on the box; no cover is cardboard, pencil label.
C3  Look at a box: "PC. Plays here or on zao. Last played 9 days ago, 7 times.", total time after it; a resumable game leans on the console with a "paused" tag.
C4  Use a box with one place: "Spelunky starts here." or "starts on zao."; two places say "Use Spelunky with" and light the TV and zao's monitor; one is picked.
C5  Open a game box: its actions are dialogue lines in the panel, as SCUMM offered choices; description on the sentence line; disabled dimmed; destructive asks.
C6  Use the typewriter alone: each letter struck darkens the boxes whose titles lack it, and the crosshair walks only the lit boxes.
C7  Busy: the chosen screen shows the kicker, the detail prints over the scene, and the host's actions (Cancel) replace the verbs as dialogue lines.
C8  Running: the room stays, the chosen screen shows the game's cover, and the sentence line holds the kicker while the game is live.
C9  Problem: the chosen screen goes to snow; "<game title>: reason" prints over the scene; dialogue lines "Try again" (only when it can) and "Leave it".
C10 Push the console: "Switch it off and on again?" with Yes and No as dialogue lines.
C11 Open the console: a route per dialogue line (runnerBuild, program), warnings above, the saved one "as before"; the state on the sentence line.
C12 Open the desk drawer: a folder per group (Device, Wi-Fi, Identity…), each folder a card of "label: value" lines; description on the sentence line.
C13 Choice: Use on a line cycles it; text and secret: the typewriter, a secret as dots on the paper; action: Use on the line; confirm: a dialogue with its title, message and confirm label beside "Leave it".
C14 Saving: the line's value types itself out with a clack until it holds; Problem: the message printed over the scene; B dismisses.
C15 Use the console: dialogue lines of what the console can do, one per device action.
C16 Use the wallet, "Make a copy of my card": the typewriter asks the password; the card comes out in a close-up with the secret printed on it and a QR square.
C17 Use the wallet, "Put a new card in": type secret and password, pick "Keep the old card" (transfer) or "Tear it up" (delete), confirm trust loss.
C18 No answer yet
C19 Look at the wallet: old cards behind the ID, one per retired key; Pick up an old card offers "Throw it away?" only after "Backed up" has been answered yes.
C20 Working: the wallet close-up with the operation on the sentence line; Problem: the message printed over the scene; Switched: "The wallet now holds <key>."
C21 The phone's display: three green bars of four; Look at says "It's on hummingbird-lake. 72% of a signal."; Wired: a cord to the wall; Offline: handset off.
C22 Use the phone: its directory, networks as lines strongest first with bars, a padlock when secured, a star when known; NoAdapter "The phone is unplugged."; scanning "Dialling…"; attempt "Connecting to X" or "X hung up: message" with "Try the password again?"; "Redial" scans.
C23 Pick a directory line and type its password on the typewriter; the last line "Unlisted number" asks the name, then the password.
C24 The F5 screen SCUMM paused on: a boxed dialog over the game, the title at the top, each control a line; a command a button, a toggle its two labels, a choice cycled left/right, a range a slider like the volume bars, dimmed with reason; Done closes.
C25 First boot is the den with the lights off: a torch lights each object, controllers on the rug as heard, the phone, the typewriter for the name, the wallet.
C26 The clock on the wall (21:47 in LED digits); the build label on the console's sticker, "korri 0.4.1 · 9a1fb22".
C27 The den idles as a SCUMM room does: a star in the window blinks and the console LED pulses; the crosshair stays; no attract.
C28 options opens the desk drawer (settings); menu jumps the crosshair to the verb grid; system does nothing; back drops what is carried, then leaves the panel.
C29 Use the typewriter: the verb grid gives way to a keyboard grid in the same panel, four rows of letters then Shift, Space, Rubout and Done, moved with the d-pad and struck with confirm; the sentence line becomes the typed line and the paper fills; B rubs out one character, Done puts the verbs back.
C30 Focus is the crosshair, always over one hotspot, verb or pocket (the sentence line names it); scenes cut, so reduced motion has nothing to remove but idles.
C31 Confirm: the kicker on the sentence line, the detail printed over the scene, the game's box lit when it has one; the host's actions replace the verbs as dialogue lines, in the host's order and words.
C32 Progress: the chosen screen draws a bar like the volume bars, filled to done over total, "1.2 of 3.4 GB" on the sentence line; with no total "1.2 GB so far" and no bar; with no progress the kicker alone.
C33 Look at a box adds "It needs a 3.4 GB download first." ("It needs a download first." with no size); the box wears a parcel sticker on the shelf.
