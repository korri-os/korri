"""Writes every component's preview file, <Name>.<layer>.preview.slint,
beside it: named preview states with fixture data (the Caliper part's
role). Run from the package: python3 preview/write-previews.py

Each state inherits ZonePreview (ui/preview-frame.slint), which feeds the
Tokens global the window's size, look and ground; the preview renderer
(clients/shell/tests/package_previews.rs) draws each state at five sizes.
The fixture words are Zone's own fixtures' (surfaces/zone/src/fixtures).
"""
import os

FILL = "x: 0; y: 0; width: parent.width; height: parent.height;"
TABS = '[{ label: "Games", active: true }, { label: "Options" }, { label: "Save data" }]'
HINTS = '{ back: "Back", confirm: "Confirm" }'
FOOT = '{ shown: true, time-label: "Time", clock: "21:47", device: "mini", signal: { shown: true, name: "hummingbird-lake", korri: true, bars: 3, strength: "72%" }, build: "korri 0.4.1 · 9a1fb22" }'
TAGS = '[{ label: "Switch", kind: "system", korri: true }, { label: "Here", kind: "here" }]'
CARDS = ('[{ title: "Celeste Classic", initials: "CC", stripes: 1, tags: [{ label: "PICO-8", kind: "system", korri: true }, { label: "Here", kind: "here" }, { label: "Continue", kind: "continue" }] },'
         ' { title: "Hollow Knight", initials: "HK", stripes: 3, tags: ' + TAGS + ' },'
         ' { title: "Tetris", initials: "T", stripes: 4, tags: [{ label: "GB", kind: "system", korri: true }, { label: "zao", kind: "remote", korri: true }] },'
         ' { title: "Spelunky", initials: "S", stripes: 2, tags: [{ label: "PC", kind: "system", korri: true }, { label: "Here", kind: "here" }, { label: "zao", kind: "remote", korri: true }] },'
         ' { title: "The Legend of Zelda: Oracle of Seasons", initials: "LZ", stripes: 5, section: "On zao", tags: [{ label: "GBC", kind: "system", korri: true }] }]')
FACTS = '[{ label: "Last played", value: "10 Oct 2026", known: true }, { label: "Plays", value: "7", known: true }, { label: "Time", value: "3h 12m", known: true }]'
ACTS = '[{ label: "Play", lit: true }, { label: "Opts." }]'
ROWS = ('[{ kind: "title", label: "Device" }, { kind: "item", label: "Name", value: "mini", value-fact: true, focused: true, bars: -1 },'
        ' { kind: "note", label: "What other devices call this one", label-fact: true },'
        ' { kind: "title", label: "Controllers" }, { kind: "item", label: "Player seats", value: "2", value-fact: true, choice: true, bars: -1 },'
        ' { kind: "title", label: "System" }, { kind: "item", label: "Power off", value: "Go", danger: true, bars: -1 },'
        ' { kind: "item", label: "Sleep", value: "Go", disabled: true, bars: -1 }, { kind: "problem", label: "That name is already used by another device on this network.", label-fact: true },'
        ' { kind: "item", label: "hummingbird-lake", label-fact: true, value: "72% Locked Saved", bars: 3, saving: true },'
        ' { kind: "buttons", buttons: [{ label: "Reload runners", focused: true }, { label: "Cancel" }] },'
        ' { kind: "word", label: "Loading" }]')
KEYS = ("[" + ", ".join(
    [f'{{ label: "{c}", row: 0, at: {i}, units: 1 }}' for i, c in enumerate("1234567890")]
    + [f'{{ label: "{c}", fact: true, row: 1, at: {i}, units: 1, focused: {"true" if c == "q" else "false"} }}' for i, c in enumerate("qwertyuiop")]
    + [f'{{ label: "{c}", fact: true, row: 2, at: {i + 0.5}, units: 1 }}' for i, c in enumerate("asdfghjkl")]
    + [f'{{ label: "{c}", fact: true, row: 3, at: {i + 1.5}, units: 1 }}' for i, c in enumerate("zxcvbnm")]
    + ['{ label: "abc", fact: true, row: 4, at: 0, units: 1, current: true }', '{ label: "ABC", row: 4, at: 1, units: 1 }',
       '{ label: "#?!", row: 4, at: 2, units: 1 }', '{ label: "://", row: 4, at: 3, units: 1 }', '{ label: "Space", row: 4, at: 4, units: 4 }',
       '{ label: "Delete", row: 4, at: 8, units: 2 }', '{ label: "Done", row: 4, at: 10, units: 2 }']) + "]")
BOARD = '{ shown: true, title: "Find", draft: "la", caret: 2, count: "2/40", rows: 5, keys: ' + KEYS + ' }'
DIALOG = '{ shown: true, headline: "Delete save", lines: ["The save on this device is removed. Saves on zao stay."], title: "Spelunky", buttons: [{ label: "No", focused: true }, { label: "Yes" }] }'
BUSY = '{ shown: true, kicker: "Starting", detail: "Waiting for zao to answer", game: "Spelunky", buttons: [{ label: "Cancel", focused: true, lit: true }] }'
PROGRESS = '{ shown: true, kicker: "Downloading FEX rootfs", game: "Hollow Knight", buttons: [{ label: "Cancel", focused: true, lit: true }], meter: true, share: 0.5, figures: "612 MB of 1.2 GB · 6.4 MB/s · 1:30 left" }'
SAVE_CARDS = ('[{ header: "Back up", word: "This key", emblem: "key", foot: "New", foot-kind: "open" }, { header: "Switch", word: "From backup", emblem: "restore" },'
              ' { header: "Switch", word: "Bunker", emblem: "link" }, { header: "Retired", word: "npub1e7k…7vj20y4z", emblem: "retired" }]')
PAUSED_ROWS = ('[{ kind: "item", label: "Continue", focused: true, bars: -1 }, { kind: "item", label: "Screen filter", value: "Sharp", value-fact: true, choice: true, bars: -1 },'
               ' { kind: "title", label: "Sound" }, { kind: "range", label: "Volume", value: "60", value-fact: true, choice: true, bars: 60 },'
               ' { kind: "item", label: "Exit", danger: true, bars: -1 }]')

def element(name, body):
    return f"{name} {{ {body} }}"

def centred(w, h):
    return f"x: (parent.width - {w}) / 2; y: (parent.height - {h}) / 2;"

A = "Tokens.a"
STATES = {
    # atoms
    "atoms/ZoneShape": [("Default", f'{centred(f"120 * {A}", f"40 * {A}")} width: 120 * {A}; height: 40 * {A}; ink: Tokens.black; commands: "M 0 0 L " + Tokens.pt(120 * {A}, 0) + " L " + Tokens.pt(100 * {A}, 40 * {A}) + " L 0 " + (40 * {A} / Tokens.unit) + " Z";', "")],
    "atoms/ZonePanel": [("Default", f'{centred(f"140 * {A}", f"14 * {A}")} width: 140 * {A}; height: 14 * {A}; run: 13 * {A}; ink: Tokens.black;', ""),
                        ("Blue", f'{centred(f"140 * {A}", f"14 * {A}")} width: 140 * {A}; height: 14 * {A}; run: 13 * {A}; ink: Tokens.blue; left-straight: true;', "")],
    "atoms/ZoneTriangle": [("Right", f'{centred(f"12 * {A}", f"13 * {A}")} width: 12 * {A}; height: 13 * {A}; ink: Tokens.red;', ""),
                           ("LeftSteel", f'{centred(f"12 * {A}", f"13 * {A}")} width: 12 * {A}; height: 13 * {A}; left: true; ink: Tokens.steel;', "")],
    "atoms/ZoneMeasure": [("Default", 'text: "MEASURED, NOT DRAWN"; cap: Tokens.c1; }\n    Rectangle { x: (parent.width - m.preferred-width) / 2; y: parent.height / 2; width: m.preferred-width; height: Tokens.c1; background: Tokens.black;', "")],
    "atoms/ZoneCaps": [("White", f'{centred("self.width", "Tokens.c2")} text: "Save data"; cap: Tokens.c2;', ""),
                       ("Hud", f'{centred("self.width", "Tokens.c1")} text: "Last played"; ink: "hud";', ""),
                       ("Danger", f'{centred("self.width", "Tokens.c2")} text: "Run"; ink: "danger"; cap: Tokens.c2;', ""),
                       ("Fact", f'{centred("self.width", "Tokens.c1")} text: "hummingbird-lake 72%"; face: "fact";', ""),
                       ("KorriName", f'{centred("self.width", "Tokens.c2")} text: "The Legend of Zelda: Oracle of Seasons"; face: "korri"; cap: Tokens.c2;', ""),
                       ("Modern", f'{centred("self.width", "Tokens.c2")} text: "Save data"; cap: Tokens.c2;', 'look: "modern";'),
                       ("Retro", f'{centred("self.width", "Tokens.c1")} text: "korri 0.4.1 · 9a1fb22"; face: "fact";', 'look: "retro";')],
    "atoms/ZoneWords": [("LongTitle", f'x: 8 * {A}; y: 8 * {A}; box: parent.width - 16 * {A}; text: "The Legend of Zelda: Oracle of Seasons"; cap: Tokens.c2; face: "korri";', ""),
                        ("LongWord", f'x: 8 * {A}; y: 8 * {A}; box: parent.width - 16 * {A}; text: "Superextraordinarilyuncompromisingprofile is a long word"; face: "fact";', "")],
    "atoms/ZoneBanner": [("Default", f'{centred("self.span", "self.height")} label: "Games";', ""), ("Long", f'{centred("self.span", "self.height")} label: "Launch settings"; focused: true;', "")],
    "atoms/ZoneTab": [("Active", f'x: (parent.width - self.span) / 2; y: parent.height / 2; tab: {{ label: "Games", active: true }}; pad-y: 5 * {A};', ""),
                      ("Focused", f'x: (parent.width - self.span) / 2; y: parent.height / 2; tab: {{ label: "Options", focused: true }}; pad-y: 5 * {A};', "")],
    "atoms/ZoneKeycap": [("BackWord", f'{centred("self.span", "self.height")} key: "B"; word: "Back"; key-ink: Tokens.key-b; words: true; pad: 5 * {A};', ""),
                         ("KeyOnly", f'{centred("self.span", "self.height")} key: "A"; word: "Confirm"; key-ink: Tokens.key-a; words: false; pad: 2 * {A};', "")],
    "atoms/ZoneBars": [("Three", f'{centred("self.width", "self.height")} lit: 3;', ""), ("Offline", f'{centred("self.width", "self.height")} lit: 0;', "")],
    "atoms/ZoneButton": [("Default", f'{centred("self.span", "self.height")} btn: {{ label: "Play" }};', ""), ("Lit", f'{centred("self.span", "self.height")} btn: {{ label: "Play", lit: true }};', ""),
                         ("Focused", f'{centred("self.span", "self.height")} btn: {{ label: "Play on zao", focused: true }};', ""),
                         ("Disabled", f'{centred("self.span", "self.height")} btn: {{ label: "Copy to zao", disabled: true }};', ""),
                         ("Destructive", f'{centred("self.span", "self.height")} btn: {{ label: "Delete", danger: true, focused: true }};', "")],
    "atoms/ZoneCoverWell": [("NoArt", f'{centred(f"60 * {A}", f"80 * {A}")} width: 60 * {A}; height: 80 * {A}; has-art: false; stripes: 3; initials: "LZ";', ""),
                            ("Tiny", f'{centred(f"20 * {A}", f"26 * {A}")} width: 20 * {A}; height: 26 * {A}; has-art: false; stripes: 0; initials: "CC";', "")],
    "atoms/ZoneEmblem": [(k.capitalize(), f'{centred(f"42 * {A}", f"32 * {A}")} width: 42 * {A}; height: 32 * {A}; kind: "{k}";', 'ground: "savedata";') for k in ["key", "restore", "link", "retired", "pad"]],
    "atoms/ZoneVeil": [("Default", FILL, ""), ("Modern", FILL, 'look: "modern";\n    Rectangle { x: 0; y: 0; width: parent.width / 2; height: parent.height; background: Tokens.white; }')],
    "atoms/ZoneTag": [(n, f'{centred("self.width", "self.height")} tag: {t};', "") for n, t in [
        ("System", '{ label: "Switch", kind: "system", korri: true }'), ("Here", '{ label: "Here", kind: "here" }'), ("Remote", '{ label: "zao", kind: "remote", korri: true }'),
        ("Continue", '{ label: "Continue", kind: "continue" }'), ("Download", '{ label: "Download 1.5 GB", kind: "download" }')]],
    "atoms/ZoneUnderline": [("Default", f'{centred(f"80 * {A}", f"20 * {A}")} width: 80 * {A}; height: 20 * {A}; run: 19 * {A};', "")],
    "atoms/ZoneVersus": [("Default", f'{centred(f"60 * {A}", f"28 * {A}")} width: 60 * {A}; height: 28 * {A};', "")],
    "atoms/ZoneQrWell": [("Default", f'{centred("min(parent.width, parent.height) - 16 * " + A, "min(parent.width, parent.height) - 16 * " + A)} width: min(parent.width, parent.height) - 16 * {A}; height: self.width; code: @image-url("../../art/veil-1.png");', 'ground: "savedata";')],
    "atoms/ZoneSignal": [("Wifi", f'{centred("self.natural", f"14 * {A}")} width: self.natural; height: 14 * {A}; signal: {{ shown: true, name: "hummingbird-lake", korri: true, bars: 3, strength: "72%" }};', ""),
                         ("Cut", f'{centred(f"80 * {A}", f"14 * {A}")} width: 80 * {A}; height: 14 * {A}; signal: {{ shown: true, name: "NETGEAR-5G-supercalifragilistic", korri: true, bars: 1, strength: "21%" }};', "")],
    # molecules
    "molecules/ZoneActionRow": [("Two", f'x: 0; y: (parent.height - self.height) / 2; width: parent.width; buttons: {ACTS};', ""),
                                ("Wraps", f'x: 0; y: (parent.height - self.height) / 2; width: parent.width; buttons: [{{ label: "Play here", focused: true }}, {{ label: "Play on zao" }}, {{ label: "Play on the-living-room-tv" }}, {{ label: "Opts." }}];', "")],
    "molecules/ZoneBusyStrip": [("Default", f'x: 8 * {A}; width: parent.width - 16 * {A}; y: parent.height - self.needed - 8 * {A}; height: self.needed; busy: {BUSY};', ""),
                                ("Progress", f'x: 8 * {A}; width: parent.width - 16 * {A}; y: parent.height - self.needed - 8 * {A}; height: self.needed; busy: {PROGRESS};', "")],
    "molecules/ZoneCard": [("Focused", f'{centred(f"104 * {A}", f"133 * {A}")} width: 104 * {A}; height: 133 * {A}; focused: true; lit: true; card: {{ title: "Hollow Knight", initials: "HK", stripes: 3, tags: {TAGS} }};', ""),
                           ("SideLongTitle", f'{centred(f"60 * {A}", f"80 * {A}")} width: 60 * {A}; height: 80 * {A}; card: {{ title: "The Legend of Zelda: Oracle of Seasons", initials: "LZ", stripes: 5, tags: [{{ label: "GBC", kind: "system", korri: true }}, {{ label: "Here", kind: "here" }}] }};', "")],
    "molecules/ZoneCardBand": [("Default", f'{FILL} cards: {CARDS}; focused-card: 1; card-focused: true; total: 5;', ""),
                               ("Last", f'{FILL} cards: {CARDS}; focused-card: 4; card-focused: true; total: 5;', ""),
                               ("OneCard", f'{FILL} cards: [{{ title: "Hollow Knight", initials: "HK", stripes: 3, tags: {TAGS} }}]; card-focused: true; total: 1;', "")],
    "molecules/ZoneDialog": [("Question", f'{FILL} dialog: {DIALOG};', ""), ("OneButton", f'{FILL} dialog: {{ shown: true, headline: "Games", lines: ["The library could not be read. The disk did not answer."], buttons: [{{ label: "Reload", focused: true }}] }};', "")],
    "molecules/ZoneFactLine": [("Default", f'x: 0; y: (parent.height - 18 * {A}) / 2; width: parent.width; height: 18 * {A}; facts: {FACTS};', ""),
                               ("NoneRecorded", f'x: 0; y: (parent.height - 18 * {A}) / 2; width: parent.width; height: 18 * {A}; facts: [{{ label: "Last played", value: "--" }}, {{ label: "Plays", value: "--" }}, {{ label: "Time", value: "--\'--\\"--" }}];', "")],
    "molecules/ZoneFoot": [("Wifi", f'x: 0; y: parent.height - Tokens.foot-h; width: parent.width; height: Tokens.foot-h; foot: {FOOT};', ""),
                           ("Offline", f'x: 0; y: parent.height - Tokens.foot-h; width: parent.width; height: Tokens.foot-h; foot: {{ shown: true, time-label: "Time", clock: "21:47", signal: {{ shown: true, name: "Offline", bars: 0 }}, build: "korri 0.4.1 · 9a1fb22" }};', "")],
    "molecules/ZoneKeyboard": [("Find", f'{FILL} board: {BOARD};', ""),
                               ("Secret", f'{FILL} board: {{ shown: true, title: "Password", name: "hummingbird-lake", draft: "•••••••", caret: 7, secret: true, count: "7/63", rows: 5, keys: {KEYS} }};', 'ground: "options";')],
    "molecules/ZoneOptionGroup": [("Default", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; title: "Saves and folders";', 'ground: "options";'),
                                  ("Caption", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; title: "Moonlight · Video"; caption: true;', 'ground: "options";')],
    "molecules/ZoneOptionRow": [("Focused", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; it: {{ kind: "item", label: "Name", value: "mini", value-fact: true, focused: true, bars: -1 }};', 'ground: "options";'),
                                ("Choice", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; it: {{ kind: "item", label: "Look", value: "Modern", focused: true, choice: true, bars: -1 }};', 'ground: "options";'),
                                ("Network", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; it: {{ kind: "item", label: "hummingbird-lake", label-fact: true, value: "72% Locked Saved", bars: 3 }};', 'ground: "options";'),
                                ("Disabled", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; it: {{ kind: "item", label: "Sleep", value: "Go", disabled: true, bars: -1 }};', 'ground: "options";')],
    "molecules/ZoneSaveCard": [("New", f'{centred(f"93 * {A}", f"133 * {A}")} width: 93 * {A}; height: 133 * {A}; focused: true; card: {{ header: "Back up", word: "This key", emblem: "key", foot: "New", foot-kind: "open" }};', 'ground: "savedata";'),
                               ("Retired", f'{centred(f"93 * {A}", f"133 * {A}")} width: 93 * {A}; height: 133 * {A}; card: {{ header: "Retired", word: "npub1e7k…7vj20y4z", emblem: "retired" }};', 'ground: "savedata";')],
    "molecules/ZoneSectionCard": [("Default", f'{centred(f"8 * {A}", f"90 * {A}")} width: 8 * {A}; height: 90 * {A}; caption: "On zao";', "")],
    "molecules/ZoneTabStrip": [("Games", f'x: 0; y: 0; width: parent.width; height: Tokens.top-h; tabs: {TABS}; tabs-shown: true; hints: {HINTS};', ""),
                               ("FocusedTab", f'x: 0; y: 0; width: parent.width; height: Tokens.top-h; tabs: [{{ label: "Games" }}, {{ label: "Options", active: true, focused: true }}, {{ label: "Save data" }}]; tabs-shown: true; hints: {HINTS};', 'ground: "options";'),
                               ("Banner", f'x: 0; y: 0; width: parent.width; height: Tokens.top-h; tabs-shown: false; banner: "Run with"; hints: {HINTS};', 'ground: "options";')],
    "molecules/ZoneTagRow": [("Card", f'{centred(f"96 * {A}", f"16 * {A}")} width: 96 * {A}; tags: [{{ label: "Spelunky", kind: "system", korri: true }}, {{ label: "Here", kind: "here" }}, {{ label: "zao", kind: "remote", korri: true }}, {{ label: "Continue", kind: "continue" }}];', ""),
                             ("Natural", f'{centred("self.natural", f"16 * {A}")} width: self.natural; fill: false; tags: [{{ label: "Saved", kind: "here" }}, {{ label: "System gba", kind: "system", korri: true }}];', "")],
    "molecules/ZoneTitleCard": [("Default", f'{FILL} title: "Hollow Knight"; system: "Switch"; act: 8;', 'ground: "titlecard";'),
                                ("LongTitle", f'{FILL} title: "The Legend of Zelda: Oracle of Seasons"; system: "GBC"; act: 1;', 'ground: "titlecard";')],
    # organisms
    "organisms/ZoneGround": [(g.capitalize(), FILL, f'ground: "{g}";') for g in ["games", "options", "savedata", "paused", "titlecard"]],
    "organisms/ZoneAttract": [("TitleTurn", f'{FILL} cards: {CARDS}; start: 1; idle-ms: 30200;', ""), ("Stage", f'{FILL} cards: {CARDS}; start: 1; idle-ms: 34000;', "")],
    "organisms/ZoneControllerCard": [("Heard", f'{centred(f"82 * {A}", f"100 * {A}")} width: 82 * {A}; height: 100 * {A}; seat: {{ seat: "1P", name: "Xbox Wireless Controller", heard: true, focused: true }};', ""),
                                     ("Waiting", f'{centred(f"82 * {A}", f"100 * {A}")} width: 82 * {A}; height: 100 * {A}; seat: {{ seat: "" }};', "")],
    "organisms/ZoneGameOpts": [("Default", f'{FILL} title: "Spelunky"; rows: [{{ kind: "item", label: "Verify files", value: "Run", focused: true, choice: true, bars: -1 }}, {{ kind: "note", label: "Check the game\'s files against the library", label-fact: true }}, {{ kind: "item", label: "Delete save", value: "Run", danger: true, choice: true, bars: -1 }}, {{ kind: "item", label: "Copy to zao", value: "Run", disabled: true, choice: true, bars: -1 }}, {{ kind: "note", label: "zao is not on the network", label-fact: true }}];', 'ground: "gameopts";')],
    "organisms/ZoneLaunchCard": [("Default", f'{FILL} card: {{ shown: true, title: "Hollow Knight", system: "Switch", act: 8 }};', 'ground: "titlecard";')],
    "organisms/ZoneOptionsEntry": [("Default", f'{FILL} board: {BOARD};', 'ground: "options";')],
    "organisms/ZonePausedControl": [("Choice", f'x: 40 * {A}; y: (parent.height - self.height) / 2; width: parent.width - 60 * {A}; height: 34 * {A}; it: {{ kind: "item", label: "Screen filter", value: "Sharp", value-fact: true, focused: true, choice: true, bars: -1 }};', 'ground: "paused";'),
                                    ("Range", f'x: 40 * {A}; y: (parent.height - self.height) / 2; width: parent.width - 60 * {A}; height: 34 * {A}; it: {{ kind: "range", label: "Volume", value: "85", value-fact: true, focused: true, choice: true, saving: true, bars: 85 }};', 'ground: "paused";')],
    "organisms/ZoneRowList": [("Options", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; height: parent.height - 16 * {A}; rows: {ROWS};', 'ground: "options";')],
    "organisms/ZoneRunWithRoute": [("Saved", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; route: {{ kind: "route", label: "RetroArch mGBA", value: "/nix/store/8n1x2-retroarch-1.19.1", value-fact: true, focused: true, tags: [{{ label: "Saved", kind: "here" }}, {{ label: "This game", kind: "system", korri: true }}] }};', 'ground: "options";')],
    "organisms/ZoneSaveBand": [("Default", f'{FILL} cards: {SAVE_CARDS}; at: 0; marked: true;', 'ground: "savedata";'),
                               ("Retired", f'{FILL} cards: {SAVE_CARDS}; at: 3; marked: true; retired: [{{ label: "Back up", focused: true, lit: true }}, {{ label: "Delete" }}];', 'ground: "savedata";')],
    "organisms/ZoneSecretSlab": [("Focused", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; height: parent.height - 16 * {A}; focused: true; secret: "ncry ptse c1wn 4zqd 7meh jmmn 4sa8 a2zy u4qm yvdk vr6z 0dxh 47p0 p2b4 nzc3 kx3j";', 'ground: "savedata";')],
    "organisms/ZoneStatusLayer": [("Busy", f'{FILL} busy: {BUSY};', ""), ("Problem", f'{FILL} dialog: {{ shown: true, headline: "Couldn\'t start", korri: true, lines: ["The emulator exited before the game loaded."], title: "Spelunky", buttons: [{{ label: "Dismiss", focused: true }}, {{ label: "Retry" }}] }};', ""),
                                  ("Running", f'{FILL} hud: {{ shown: true, kicker: "Playing on zao", title: "The Legend of Zelda: Oracle of Seasons" }};', ""), ("Progress", f'{FILL} busy: {PROGRESS};', "")],
    "organisms/ZoneStepStrip": [("Second", f'x: 0; y: (parent.height - self.height) / 2; width: parent.width; pip: 1;', "")],
    "organisms/ZoneTally": [("Counted", f'{FILL} tally: {{ shown: true, title: "Hollow Knight", counted: true, rows: [{{ label: "Time", value: "33\'05\\"", share: 0.15 }}, {{ label: "Plays", value: "8", share: 1 }}, {{ label: "Total", value: "3H 45M", share: 1 }}] }};', 'ground: "tally";')],
    "organisms/ZoneWarningLines": [("Warning", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; warning: true; line: "video.shader · retroarch-mgba · 1.19.1: the shader preset was not found and the default is used";', 'ground: "options";'),
                                   ("Message", f'x: 8 * {A}; y: 8 * {A}; width: parent.width - 16 * {A}; line: "Choose a runner on this device. Launch once does not change saved choices.";', 'ground: "options";')],
    "organisms/ZoneSkyBand": [("Head", f'{FILL} head: {{ shown: true, title: "Wario Land 4", tags: [{{ label: "Ready", kind: "system" }}] }}; rows: {ROWS};', 'ground: "options";'),
                              ("Entry", f'{FILL} head: {{ shown: true, title: "Celeste Classic 2", tags: [{{ label: "PICO-8", kind: "system", korri: true }}] }}; well: {{ shown: true, initials: "CC", stripes: 1 }}; rows: [{{ kind: "text", label: "Madeline climbs again, with a grappling hook this time.", label-fact: true }}, {{ kind: "buttons", buttons: [{{ label: "Get", focused: true, lit: true }}] }}];', 'ground: "options";')],
    # template
    "templates/ZoneShell": [("Tabs", f'{FILL} tabs: {TABS}; foot: {FOOT};', ""), ("Banner", f'{FILL} tabs-shown: false; banner: "Get games"; foot: {FOOT};', 'ground: "options";'),
                            ("Steps", f'{FILL} tabs-shown: false; banner: "Controllers"; pips: 4; pip: 0;', "")],
}

PAGES = {
    "ZoneGames": [("Ready", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Ready"; cards: {CARDS}; focused-card: 1; card-focused: true; total: 5; facts: {FACTS}; actions: {ACTS}; find-shown: true; get-shown: true;', ""),
                  ("Loading", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Loading";', ""),
                  ("Empty", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Empty";', ""),
                  ("Find", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Ready"; cards: {CARDS}; total: 5; facts: {FACTS}; actions: {ACTS}; typing: true; keyboard: {BOARD};', ""),
                  ("Modern", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Ready"; cards: {CARDS}; focused-card: 3; card-focused: true; total: 5; facts: {FACTS}; actions: [{{ label: "Play here", focused: true }}, {{ label: "Play on zao" }}, {{ label: "Opts." }}]; find-shown: true; kept: "la";', 'look: "modern";'),
                  ("Retro", f'{FILL} tabs: {TABS}; hints: {HINTS}; foot: {FOOT}; catalog: "Ready"; cards: {CARDS}; focused-card: 1; card-focused: true; total: 5; facts: {FACTS}; actions: {ACTS}; find-shown: true;', 'look: "retro";')],
    "ZoneOptions": [("Default", f'{FILL} tabs: [{{ label: "Games" }}, {{ label: "Options", active: true }}, {{ label: "Save data" }}]; hints: {HINTS}; foot: {FOOT}; rows: {ROWS};', 'ground: "options";'),
                    ("Keyboard", f'{FILL} tabs: [{{ label: "Games" }}, {{ label: "Options", active: true }}, {{ label: "Save data" }}]; hints: {HINTS}; foot: {FOOT}; rows: {ROWS}; keyboard: {BOARD};', 'ground: "options";')],
    "ZoneRunWith": [("Ready", f'{FILL} hints: {HINTS}; foot: {FOOT}; head: {{ shown: true, title: "Wario Land 4", tags: [{{ label: "Ready", kind: "system" }}] }}; rows: [{{ kind: "text", label: "Choose a runner on this device. Launch once does not change saved choices.", label-fact: true }}, {{ kind: "route", label: "RetroArch mGBA", label-fact: true, value: "/nix/store/8n1x2-retroarch-1.19.1", value-fact: true, focused: true, tags: [{{ label: "Saved", kind: "here" }}, {{ label: "System gba", kind: "system", korri: true }}] }}, {{ kind: "buttons", buttons: [{{ label: "Launch once" }}, {{ label: "Remember for this game" }}] }}];', 'ground: "options";')],
    "ZoneStore": [("Results", f'{FILL} hints: {HINTS}; foot: {FOOT}; head: {{ shown: true, title: "PICO-8 BBS" }}; rows: [{{ kind: "title", label: "Search" }}, {{ kind: "item", label: "Search for", value: "cel", value-fact: true, bars: -1 }}, {{ kind: "title", label: "Results" }}, {{ kind: "item", label: "Celeste Classic 2", label-fact: true, value: "PICO-8", value-fact: true, focused: true, bars: -1 }}];', 'ground: "options";')],
    "ZoneLaunchSettings": [("Ready", f'{FILL} hints: {HINTS}; foot: {FOOT}; head: {{ shown: true, title: "Wario Land 4", tags: [{{ label: "Ready", kind: "system" }}] }}; rows: [{{ kind: "title", label: "Runner" }}, {{ kind: "item", label: "Runner scope", value: "mGBA", focused: true, choice: true, bars: -1 }}, {{ kind: "title", label: "Device" }}, {{ kind: "caption", label: "Moonlight · Video" }}, {{ kind: "item", label: "FPS", value: "60", value-fact: true, bars: -1 }}, {{ kind: "buttons", buttons: [{{ label: "Reload" }}, {{ label: "Close" }}] }}];', 'ground: "options";')],
    "ZoneSaveData": [("Band", f'{FILL} tabs: [{{ label: "Games" }}, {{ label: "Options" }}, {{ label: "Save data", active: true }}]; hints: {HINTS}; foot: {FOOT}; cards: {SAVE_CARDS}; marked: true;', 'ground: "savedata";'),
                     ("Backup", f'{FILL} tabs: [{{ label: "Games" }}, {{ label: "Options" }}, {{ label: "Save data", active: true }}]; hints: {HINTS}; foot: {FOOT}; qr-shown: true; qr: @image-url("../art/veil-1.png"); secret: "ncry ptse c1wn 4zqd 7meh jmmn 4sa8 a2zy"; slab-focused: true;', 'ground: "savedata";')],
    "ZoneOnboarding": [("Controllers", f'{FILL} hints: {HINTS}; banner: "Controllers"; pip: 0; seats: [{{ seat: "1P", name: "Xbox Wireless Controller", heard: true, focused: true }}, {{ seat: "2P", name: "8BitDo Pro 2", heard: true }}]; versus: true; seat-name: "Xbox Wireless Controller"; next: {{ label: "Next", lit: true }};', ""),
                       ("Identity", f'{FILL} hints: {HINTS}; banner: "Identity"; pip: 3; new-save: {{ shown: true, focused: true }}; next: {{ label: "Finish", disabled: true }};', "")],
    "ZonePaused": [("Default", f'{FILL} rows: {PAUSED_ROWS}; hints: {HINTS};', 'ground: "paused";'),
                   ("Question", f'{FILL} rows: {PAUSED_ROWS}; hints: {HINTS}; dialog: {{ shown: true, headline: "Restart", buttons: [{{ label: "No", focused: true }}, {{ label: "Yes" }}] }};', 'ground: "paused";')],
}

SUFFIX = {"atoms": "atom", "molecules": "molecule", "organisms": "organism", "templates": "template", "pages": "page"}

def write(path, name, states, depth):
    up = "../" * depth
    lines = [f"// {name}'s preview states, with fixture data from Zone's fixtures.",
             "// Written by preview/write-previews.py.", "",
             f'import {{ Tokens }} from "{up}ui/tokens.slint";',
             f'import {{ ZonePreview }} from "{up}ui/preview-frame.slint";',
             f'import {{ {name} }} from "./{name}.slint";', ""]
    for state, body, frame in states:
        lines.append(f"export component {state} inherits ZonePreview {{")
        if frame:
            lines.append(f"    {frame}")
        lines.append(f"    {'m := ' if name == 'ZoneMeasure' else ''}{name} {{ {body} }}")
        lines.append("}")
        lines.append("")
    open(path, "w").write("\n".join(lines))

for key, states in STATES.items():
    layer, name = key.split("/")
    write(f"ui/{layer}/{name}.{SUFFIX[layer]}.preview.slint", name, states, 2)
for name, states in PAGES.items():
    write(f"pages/{name}.page.preview.slint", name, states, 1)
print(len(STATES) + len(PAGES), "preview files")
