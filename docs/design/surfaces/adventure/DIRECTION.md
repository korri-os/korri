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
