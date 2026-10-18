# Workbench: Korri as Workbench 2.x, documents in windows

## Colour: the four Workbench 2.x pens, nothing else
- Workbench grey `#AAAAAA`: pen 0, the desktop and every window face
- Detail black `#000000`: pen 1, outlines, text, the shadow edge of a bevel
- Shine white `#FFFFFF`: pen 2, the screen title bar, the lit edge of a bevel
- Window blue `#6688BB`: pen 3, the active title bar and the selected icon
- Bevels: 1px white over 1px black, or the reverse when sunk. No blur.

## Type
Spleen 5x8 (BSD 2-Clause), inlined, at 8px: one glyph pixel is one screen
pixel. It has the same 5x8 monospace cell as Topaz 8. One face, one size. The
screen zooms by a whole number, so text stays crisp. Pico's Spleen build draws
`·` empty, so the build label paints its dot.

## Skeleton (one sentence)
Drawer icons (Games, Prefs) on a grey desktop open into gadgeted windows,
under a screen bar that holds the build label, a Wi-Fi tool and the clock.

## Layout: `plan(width, height)` in index.html
Scale is the largest whole number that keeps a 320x240 screen, one step less
if that buys room (560x340) to overlap. Overlap: windows overlap. Stack: the
front window is full width, other open windows dock as title bars at the
bottom edge, and confirm on a bar brings it forward. Open windows, depth order
and the selected icon survive a resize.

## Memorable element
Workbench's complement selection (pen n to pen 3-n) turns the Spelunky box
blue and white in a sunk frame; the Info line reads "Play on: This device, zao".

## Text entry
A string gadget is the field. Confirm on it opens a Keyboard window of key
gadgets under it (docked in stack mode). The d-pad moves between keys; confirm
types at the gadget's cursor. Up from the top key row enters the gadget, where
left and right move the cursor; down returns to the keys. Back deletes before
the cursor, and closes the keyboard on an empty gadget. A 40-character
identity string scrolls inside the gadget, cursor kept in view, with "17/40".
Shift, Space, Cancel and Use end the last row; Use sends, Cancel keeps the old.
