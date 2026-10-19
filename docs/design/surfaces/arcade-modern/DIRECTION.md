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


## Capability map

C1  Loading: empty ring, NOW LOADING on the plinth; Error: extruded ERROR headline, message in cyan, RESET lit below; Empty: NO GAMES (drawn); Ready: the ring
C2  Numbered ring cards in catalog order; a section opens with a cyan tab on its first card; subtitle under the panel title; covers letterboxed, no cover = house card
C3  RANKING panel: PLAYS, LAST PLAYED and a PLAY TIME line; the card's red CONTINUE flag marks resumable
C4  A on a one-cabinet game plays it; a linked game drops into the LINK · 2 CABINETS pair, A plays on the lit side (1P or 2P SIDE); no side is picked for the person
C5  options on the focused card opens its GAME TEST MODE: a service list of gameActions, dimmed when disabled, description under the lit row; destructive ask YES / NO
C6  Up from the ring raises the name-entry panel as a filter: each letter trims the ring to matching titles, ENTRY counts the matches; B clears and closes it
C7  NOW LOADING on the plinth with the kicker, detail in cyan, the game's card alone on the ring; host actions (CANCEL) as yellow-block items on the service line
C8  The showcase keeps the running game's card, its extruded title reads the kicker (NOW PLAYING) and the ring stops turning
C9  An ERROR headline in the showcase: kicker, reason in chrome, gameTitle in yellow; RETRY (when canRetry) and DISMISS as a lit pair like the cabinets
C10 RESET as the last item of the TEST MODE line, lit by default when the catalog is in Error; A runs reload()
C11 No answer yet
C12 TEST MODE › SETTINGS: a service page in the panel's idiom, each group under a RANKING-style header, label in cyan, value in chrome, description under the lit row
C13 Choice: left/right cycle the value in the yellow block; text/sensitiveText raise the name-entry panel (dots, clearLabel as a key); action: A fires, after YES / NO when it confirms
C14 Saving: the row's value blinks NOW WRITING in yellow; Problem: a red message line under the row with DISMISS lit
C15 Device actions head the SETTINGS page as a SYSTEM run above the groups, each a service item; system jumps straight to it
C16 TEST MODE › IDENTITY › BACKUP: the name-entry panel asks the password, then the showcase card shows the encryptedSecret as a QR with the text under it
C17 RESTORE: the name-entry panel takes the secret then the password, TRANSFER / DELETE as a 1P / 2P SIDE pair, then YES / NO under THIS CABINET LOSES TRUST
C18 No answer yet
C19 RETIRED KEYS: a RANKING-style list of retiredPublicKeys; DELETE on a key asks BACKUP CONFIRMED? YES / NO first
C20 Working: NOW LOADING with the operation; Problem: the ERROR headline with the message; Switched: SWITCHED headline over the new ownerPublicKey; B dismisses
C21 Signal badge top-right: four bars and SSID 72% for Wifi; LINK with full bars for Wired; empty bars and NO LINK for Offline
C22 TEST MODE › NETWORK: networks strongest first with bars, a lock when secured, KNOWN tag; SCANNING… readout; NO NETWORK BOARD when NoAdapter; attempt line under the list, a rejected key reopens the panel
C23 A on a row joins it, the name-entry panel asking a password when needed; HIDDEN NETWORK is the last row and asks SSID then password
C24 TEST during play opens GAME TEST MODE over the game: overlay controls, then each group as a sub-page of cycling values; dimmed rows show why, destructive ask YES / NO; EXIT dismisses
C25 First power-on runs the cabinet's SYSTEM CHECK: INPUT TEST (a controller's row lights when heard), NETWORK, CABINET NAME, IDENTITY; EXIT is finishOnboarding
C26 CREDIT 21:47 top-left is clockLabel (hidden when absent); buildLabel sits at the foot's right after the mini tag
C27 Attract: 20 s idle on SELECT cuts to the attract reel; any press returns to SELECT with the previous focus intact
C28 menu jumps to TEST MODE; options opens the focused game's GAME TEST MODE; system lands on SETTINGS' SYSTEM run; d-pad down reaches all of it: ring › places › service
C29 The name-entry panel from the foot: slanted cyan-framed keys, yellow block cursor, A types, B deletes, options shifts, menu confirms; yellow entry bar with 12/40 count, dots for a password
C30 Focus is always the yellow block, the yellow card frame or the cabinet outline; ring rotation eases 0.45 s; prefers-reduced-motion removes attract, pulse, sweep and wipe
