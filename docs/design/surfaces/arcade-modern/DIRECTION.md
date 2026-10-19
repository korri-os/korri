# Arcade-modern: a Naomi-era cabinet's attract and select screens

**Skeleton.** Two first-class states: ATTRACT (a reel of 3D-tilted title cards over the library, `?attract`) and SELECT (the delivered shot): a rotating ring of twelve tilted cards with the focused one large on a lit plinth under its extruded title, a RANKING panel on the left with an angled edge and light bar, the linked-cabinet places pair, and the TEST MODE service line at the foot.

**Palette.** Board blue `#1322C8` (ground, top of the gradient), pit navy `#06073F` (ground's deep end, card grounds, panel), hot orange-red `#FF3A1E` (extrusion, flags, stripe, TEST MODE), chrome white `#F4F6FF` (type, chrome top), start yellow `#FFD41A` (focus, plinth light bar, PRESS START, RANKING), sweep cyan `#5CF0FF` (light sweeps, labels, signal badge, card frames). Gradients and glows are hard-banded (chrome fill: white / cyan / white / yellow / red with sharp stops).

**Type.** Barlow Condensed Black Italic (display: titles, RANKING, PRESS START, readouts) and Barlow Condensed SemiBold (facts, panel values). Both SIL OFL 1.1, fetched from google/fonts `ofl/barlowcondensed`, embedded base64; licence copied to `LICENSE-BarlowCondensed-OFL.txt` and `fonts/OFL.txt`. The face has a real italic, so no skew is applied.

**Memorable element.** The focused showcase card: tilted `rotateY(-14deg) rotateX(5deg)`, cyan-then-yellow frame, hard diagonal specular, a moving light sweep, on a yellow-lit plinth, with the title in chrome-banded display type extruded nine steps in red to navy. Everything else (ring, panel, foot) is quieter.

**plan(width, height)** (one function in the file, all thresholds named there):
- `side = w >= 900 && w/h >= 1.35`; side mode puts the panel left at `clamp(30% w, 300, 440)`; band mode drops it under the showcase at `clamp(30% h, 140, 190)` with SYSTEM · PLAYS · LAST on one line and the places pair under it.
- `top = clamp(6% h, 30, 48)`, `foot = clamp(5.5% h, 28, 44)` × `footRows` (`2` when `w < 600`: service items on line one, `mini` + version on line two).
- `visible = showW >= 760 ? 12 : 5` (focused ± 2 neighbours when 5); ring step 30° (12) or 55° (5); `rx = .42 showW`, `ry = .24 showH`.
- `cardW = min(showW × (w/h < 1 ? .52 : .36), showH × .54 × 4/3)`, 4:3 cards; title `clamp(.2 cardW, 26, 72)` and shrinks by 2px until its lines fit under the card (Zelda breaks after the colon).
- Checked at 1280×720, 640×480, 1920×1080, 480×800; stress set `?n=0`, `?n=1`, `?n=40`, `?focus=9` (Zelda), `?focus=6` (Comet Courier), `?focus=5` (Tide Pool: 0 and NEVER), `?zone=places|service`.

**Focus model.** Ring: left/right rotate (wrap), down enters the places pair, A on a two-place game enters the pair. Places: left/right switch 1P SIDE / 2P SIDE, A plays on the lit side, up or B returns to the ring, down reaches TEST MODE. TEST MODE: left/right across SETTINGS · NETWORK · IDENTITY (yellow block), A opens, up or B back; `menu` jumps here from anywhere. Keys in the file: arrows, Enter/Z = A, Backspace/X = B, M = menu, O = options.

**Attract loop.** 20 s idle on SELECT cuts to ATTRACT at entry 1. Each card: 0.7 s diagonal red/yellow wipe, 0.6 s card sweep-in from bottom-left with rotateY easing, hold 4 s with the RANKING line (SYSTEM, PLAYS, LAST), then the next in catalog order; PRESS START pulses at 1 Hz bottom-centre; CREDIT 21:47 top-left and the signal badge top-right stay. Any press returns to SELECT with the previous focus intact. Select-screen motion: ring rotation eases 0.45 s, the focused card's light sweep loops every 5.5 s, the CONTINUE flag glows at 1.2 s. `?still` freezes all of it (sweep parked at 60%); `prefers-reduced-motion` removes the attract timer, the pulse, the sweep and the wipe.

**Decisions made here.** Clock readout is `CREDIT 21:47` (not TIME). PRESS START appears only in ATTRACT; SELECT's foot carries the service line. The catalog position is shown as `ENTRY 4 / 12` in the panel (it is the card number, not an invented fact). Comet Courier's house card: navy, red diagonal band, chrome-banded title, light sweep. Petal Quest, Zelda, Wario Land 4 and Super Metroid get flat vector covers in that same idiom; the seven fixture pictures are letterboxed on the card's navy ground, never cropped.

**Text entry.** A name-entry panel slides up from the foot in the showcase's style: a slanted on-screen keyboard (letters, digits, symbols, SPACE, END) in cyan-framed keys; d-pad moves across the keys with the yellow block as cursor, A types, B deletes, `options` toggles upper/lower/symbols, `menu` confirms; the typed string sits in a lit yellow entry bar above the keys with the count (`12/40`) at its right, masked as dots for a Wi-Fi password with `options` held to reveal.
