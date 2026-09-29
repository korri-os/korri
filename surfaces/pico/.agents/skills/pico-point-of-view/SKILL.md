---
name: pico-point-of-view
description: Pico's point of view, the intent behind every Pico screen. Load before you plan, change or review any Pico UI, including layout, colour, motion, game art, copy and new screens.
---

# Pico's point of view

Pico is a toy console. Games are cartridges you pick up, and the console is
playful and physical.

This file holds intent. The code holds the current design: values, components
and layouts. Read those in the code. When this file and the code disagree
about intent, this file wins. When they disagree about a value, the code wins.

## Who Pico is for

Pico is a first-party Korri surface, and it covers everything Korri can do. A
person chooses it from a list of first-party and third-party surfaces, so its
character must be worth choosing.

Its look suits small handhelds best. That is the only difference. On a large
screen, Pico offers every capability it offers on a small one.

## Priorities

When ideas compete, rank them in this order:

1. **Surprise and delight.** Hidden details, idle animations and secrets that
   reward a player who waits or looks closely.
2. **The voice.** A dry narrator. See "Voice".
3. **Objects that behave like toys.** Things click, slot in, pop out and have
   weight.

The voice stays dry, so surprise comes mostly from objects and visuals.

## Identity

These rules make Pico a fantasy console. They stay fixed when the design
changes.

- **The palette.** Pico draws its own interface in the 16 PICO-8 colours.
  Colours meet at hard edges, and each area is one solid palette colour.
- **The pixel grid.** Every size is a whole number of one virtual pixel. Every
  edge lands on the grid and renders sharp.
- **Stepped motion.** Things move in discrete frames, like a sprite.
- **Pixel text.** Text is a bitmap face on the same grid. The choice of face is
  a design choice.
- **Games are objects.** Every game reads as a physical thing you hold.
  Cartridges are today's object. A later design may choose another one.

## Game art

Game art comes from outside Pico, so it has its own rule. Pixelate it onto
Pico's grid and push it toward vivid colour. It may use more than 16 colours.
It must read as pixel art: sharp pixels, strong saturation, clear contrast.

## Delight and speed

Delight waits for the player. It runs when the player is idle or has finished
an action, and any press ends it at once.

One exception: a short, deliberate moment of about one second may delay input
if it plays once per session and any press skips it. Use it rarely.

## Voice

Pico speaks as a dry narrator, in short, flat lines. It states what is on the
screen. Most lines carry no joke. When a joke appears, it lands on the console
or its objects. The player is never the target. Pico never says "I".

The register, not lines to reuse:

| Moment | Line |
|---|---|
| Empty library | "Shelf empty. It's taking it well." |
| Search finds nothing | "Nothing called that. Try fewer letters." |
| Player returns after idle | "Still here. So is everything else." |
| Setting saved | "Done." |

The voice covers Pico's own words: labels, empty states, idle screens and
hints. Korri supplies finished text for status and errors. Show that text
exactly as Korri sends it. Show only facts that Korri publishes.

## References

Take the lesson from each one, not its look:

- **PICO-8.** A strict palette can carry a strong personality.
- **Playdate.** A toy console can feel new instead of nostalgic.
- **Nintendo's hidden details** (Wii and 3DS menus, Animal Crossing). Idle
  details and secrets reward players who wait or look closely.

## Neighbours to steer away from

Each neighbour pulls a take toward the most probable answer. Aim at Pico's
answer instead.

| Neighbour | Its pull | Pico's answer |
|---|---|---|
| Shift, Korri's cinematic surface | Full-screen art scenes and atmosphere | Objects on a flat ground of palette colour |
| Modern console dashboards | Tiles, glass, blur, soft shadows, rounded cards | Hard-edged objects on the pixel grid |
| Retro nostalgia | CRT scanlines, synthwave glow, screen curve | A crisp console that never existed |
| Parody | "PRESS START", "1UP" and coin sounds in place of content | Real content, with dry wit |
| Developer tools | Terminal, monospace and DOS looks | A toy |

## Reject a take when

Each item is a yes-or-no check. One yes rejects the take.

1. Pico's own drawing uses a colour outside the palette, or a blend, a
   gradient or partial opacity.
2. Anything sits off the pixel grid or renders blurred, including scaled game
   art.
3. Any motion eases instead of stepping.
4. A game does not read as an object.
5. Game art looks smooth, photographic or washed out.
6. A Korri capability is removed, hidden or made harder to reach.
7. Pico rewrites text that Korri supplied, or shows a fact that Korri does not
   publish.
8. Delight delays input, apart from the one skippable moment per session.
9. The take moves toward a neighbour in "Neighbours to steer away from".
