# Arcade-modern: a Naomi-era cabinet's attract and select screens

**Skeleton.** Two first-class states: ATTRACT (a reel of 3D-tilted title cards over the library, `?attract`) and SELECT (the delivered shot): a true 3D ring of twelve cards with the focused one large on a lit plinth band under its extruded title, a RANKING card floating top-left over the stage, and the TEST MODE service line at the foot.

**Palette.** Board blue `#1322C8` (ground, top of the gradient), pit navy `#06073F` (ground's deep end, card grounds, panel, hard shadows), hot orange-red `#FF3A1E` (extrusion, flags, panel light bar, TEST MODE), chrome white `#F4F6FF` (type, chrome top, inner frame line), start yellow `#FFD41A` (focus, plinth bar, PRESS START, number tab), sweep cyan `#5CF0FF` (light sweeps, labels, RANKING tab, card frames, ring track at 25%). Gradients and glows are hard-banded.

**Type.** Barlow Condensed Black Italic (display: titles, tabs, readouts, PRESS START) and Barlow Condensed SemiBold (facts, panel values, entry readout). Both SIL OFL 1.1, from google/fonts `ofl/barlowcondensed`, embedded base64; licence in `LICENSE-BarlowCondensed-OFL.txt` and `fonts/OFL.txt`. Display tracking: 0 on titles, +.02em on small caps readouts (`.caps`), never wider. Labels and values share one size; labels cyan, values chrome; `tabular-nums` everywhere.

**Memorable element.** The focused showcase card: tilted `rotateY(-14deg) rotateX(5deg)`, yellow frame with a 1px chrome inner line, hard diagonal specular, a moving light sweep, yellow number tab top-left, standing on a plinth band that carries the title in chrome-banded display type extruded nine steps in red to navy. Letterboxed pictures sit on a faint diagonal hatch.

**Panel order (side mode).** RANKING is a cyan tab on the top edge, not a heading. Then: title (display face, chrome-banded, 36px·kt, one line shrinking to 72%, then two lines max) · facts table (SYSTEM / PLAYS / LAST PLAYED; label column max-content, values on one left axis) · places (only when shown, see below) · `ENTRY 4 / 12` in the facts face at 70%. Width `clamp(26% w, 280, 380)`, height fits content, one left margin (`--sp-24`), everything on the 4px grid (`--grid`). The ring's back-left cards pass behind it; the panel is opaque navy.

**Disclosure rule.** Resting SELECT shows the showcase, ring, top readouts and the panel with title + facts only. The places pair appears as a one-line hint `LINK · 2 CABINETS ▾` only when the focused game has two places and focus is on the ring; it expands into the 1P/2P cabinet boxes only when focus enters it (`?zone=places`, down or A). TEST MODE sits at 55% and lights to full when focused or `menu` is pressed (`?zone=service`). On the ring only the focused card and its two nearest neighbours carry a readable title; farther cards show number and picture only.

**Ring math (one line).** Cards sit on an ellipse `(rx, ry = .2·showH)` at angle `θ(k)`: the front five (k = −2..2) at `k·stepF`, the back cards sharing the rest of the circle at `stepB = (360 − 4·stepF)/(N − 4)`; screen point `X = cx + rx·sin θ, Y = cy − ry·(cos stepF − cos θ)` so k = ±1 sit level with the showcase centre; depth `d = (1 − cos θ)/2` gives `translateZ = −1000·d`, `rotateY = 50·sin θ`, navy shade `.55·d`, `z-index ∝ 1 − d`; `translate3d` is solved so the perspective (1100px, origin at .35·cy) projects each card onto that point. Side mode: the ring region is `[panelRight + sp24, w − sp24]`, `cx` is its centre (showcase too), showcase `cardW = .36·region` (315 at 1280), `rx = .42·region` growing to the region edge, then `stepF` (30°..48°) is solved so the front five keep a 16px gap from the showcase frame and each other; only past 48° does the showcase shrink. Back-half cards (|θ| > 90°) may pass behind the showcase and plinth but their number tab stays visible (the ellipse nudges up by any overlap); no card ever sits under the panel. Band mode: 5 cards at 55°, `rx` the smallest radius that clears the showcase by 8px, never past the viewport edge. Back cards are higher on screen (ring seen from above) so the plinth band stays clear; the thin cyan track is the same ellipse. Step 30° for 12 cards (360/N for 8..12), 55° for the 5-card ring (`w < 760`); N > 12 shows 12 around the focus.

**Spacing scale.** `--sp-4 … --sp-48` = {4, 8, 12, 16, 24, 32, 48} × `k`, `k = max(.6, min(w/1280, h/720))`, rounded, written by `plan()`; type uses `--kt = min(k, 1.2)`. Top and foot bars are `--sp-48`, panel padding `--sp-24`/`--sp-32`, card→band `--sp-8`, band inset `--sp-12`, foot items `--sp-24` apart. The top bar carries a 3px yellow→cyan rule across the full width; the signal badge shares CREDIT's baseline.

**plan(width, height, n)** (one function in the file, thresholds named there): `side = w ≥ 900 && w/h ≥ 1.35`; band mode drops the panel under the stage as one strip whose height is its content (the ring shrinks when the pair expands); foot rows `2` when `w < 600`; `cardW = min(W·frac, H·.54·4/3)` with frac .26 side / .36 band / .48 portrait; title `clamp(.2 cardW, 26, 72)`, one line shrinking to 22px, then two lines (Zelda breaks after the colon). Checked at 1280×720, 640×480, 1920×1080, 480×800; stress `?n=0|1|40`, `?focus=9|6|5`, `?zone=places|service`.

**Focus model.** Ring: left/right rotate (wrap), down or A on a two-place game enters the places pair, down on a one-place game goes to TEST MODE. Places: left/right switch 1P SIDE / 2P SIDE, A plays on the lit side, up or B returns to the ring, down reaches TEST MODE. TEST MODE: left/right across SETTINGS NETWORK IDENTITY (yellow block), A opens, up or B back; `menu` jumps here from anywhere. Keys: arrows, Enter/Z = A, Backspace/X = B, M = menu, O = options.

**Attract loop.** 20 s idle cuts to ATTRACT at entry 1: 0.7 s wipe, 0.6 s card sweep-in, 4 s hold with the RANKING line, then the next. The card's shadow is a hard pit-navy offset; the title sits fully below the card's projected bottom edge with an sp16 gap and the RANKING line sp12 under it. PRESS START pulses at 1 Hz; no other caption. Any press returns to SELECT with focus intact. `?still` freezes everything; `prefers-reduced-motion` removes the timer, pulse, sweep and wipe.

**Decisions made here.** Clock readout is `CREDIT 21:47`. The catalog position is `ENTRY 4 / 12`, quiet, last in the panel. Comet Courier's house card: navy, red band, chrome-banded title. Petal Quest, Zelda, Wario Land 4 and Super Metroid get flat vector covers in that idiom; the seven fixture pictures are letterboxed, never cropped.

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
