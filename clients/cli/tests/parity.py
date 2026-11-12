#!/usr/bin/env python3
"""ADR 0031 inventory guard, not a Rust parser or an RPC behaviour test.

korrid builds every settings row a surface shows (services/korrid/src/surface/).
Check its row and action ID literals, constants and constructor prefixes against
the CLI commands below. Dynamic rows use prefixes. An unfamiliar expression
fails closed: when a producer changes shape, review its command instead of
silently skipping it.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
SURFACE = ROOT / "services/korrid/src/surface"

# Existing row IDs, not a new product manifest. Read-only rows still name the
# command that reads them. Every editable row needs a command or a named gap.
MAPPINGS = {
    "device-name": "devices rename NAME",
    "person-name": "identity name NAME",
    "setup-restart": "setup restart",
    "host.preferences.playerCount": "controllers seats COUNT",
    "steamgriddb-credential": "metadata key set|clear",
    "local-games": "library games list",
    "game-discovery-status": "library folders list",
    "library-folder-rescan": "library folders rescan",
    "library-folder:": "library folders list",
    "library-folder-remove:": "library folders remove ID",
    "korrid-version": "status",
    "plugin-repository-add": "plugins repositories add URL",
    "plugin-repository:official": "plugins repositories list",
    "plugin-repository:": "plugins repositories list",
    "plugin-repository-remove:": "plugins repositories remove URL",
    "plugin-release:": "plugins review|install URL ID VERSION",
    "plugin-inspect:": "plugins review URL ID VERSION",
    "plugin-review:": "plugins install URL ID VERSION --approval DIGEST",
    "plugin-review-publisher:": "plugins review URL ID VERSION (read-only publisher facts)",
    "plugin-trust:": "plugins trust NAMESPACE --key FULL_PUBLIC_KEY; plugins review URL ID VERSION trust prompt",
    "plugin-publisher:": "plugins publishers",
    "plugin-publisher-revoke:": "plugins revoke NAMESPACE",
    "plugin-publishers:status": "plugins publishers (read-only status)",
    "plugin-toggle:": "plugins on|off ID",
    "launch-device:": "launch-settings set KEY=VALUE",
    "launch:row:": "launch-settings set KEY=VALUE [layer and scope]; play GAME --set KEY=VALUE",
    "launch:stream:": "launch-settings set --runner ID KEY=VALUE",
    "launch:keep": "play GAME --set KEY=VALUE --keep[=PLACE]; launch-settings set KEY=VALUE [layer]",
    "launch:profile:pick": "launch-settings profiles use ID|--none --game GAME",
    "launch:profile:new": "launch-settings profiles new TITLE --game GAME",
    "launch:profile:remove:": "launch-settings profiles delete ID --game GAME",
    "launch:profile:rename:": "launch-settings profiles rename ID TITLE --game GAME",
    "launch:reload": "launch-settings show --game GAME",
    "launch-device:retry": "launch-settings show",
    # Device-page plugin settings (Moonlight runner, Sunshine service), ADR 0037/0038.
    "launch-device:set:": "launch-settings set [--runner ID] KEY=VALUE|KEY --inherit|KEY --default",
    "launch:runner": "play GAME --runner ID; launch-settings runner ID --game GAME",
    "launch:retry": "repeat the failed launch-settings command after inspection",
    "launch:cancel": "no effect; exit the editor",
}

# Surface-owned preferences have no shared RPC or agreed command yet. Do not
# invent one to make this inventory green (see PARITY.md, schema unresolved).
KNOWN_GAPS = {
    "pico.font": None,
    "zone.look": None,
}

# Surface preference keys, as the native packages declare them.
PREFERENCES = {
    "pico.font": "clients/shell/packages/pico/src/pico-font-preference.ts",
    "zone.look": "clients/shell/packages/zone/logic.ts",
}

# The ID constants each producer defines. Other string constants there are
# labels or codes, never row IDs.
ID_CONSTANT = re.compile(r'\bconst ([A-Z_]+(?:_ROW|_ACTION|_SETTING|_ACTION_ID)): &str = "([^"]*)";')


def literal_id(expression, source):
    expression = expression.strip()
    if expression.startswith('"'):
        return expression[1:-1]
    formatted = re.fullmatch(r'format!\("([^"{]*)\{[^"]*"(?:,.*)?\)', expression, re.S)
    if formatted:
        return formatted[1]  # dynamic ID prefix, never broad wildcard
    constant = re.search(rf'\bconst {re.escape(expression)}: &str = ("[^"]*");', source)
    if constant:
        return literal_id(constant[1], source)
    raise AssertionError(f"Unreviewed settings ID expression: {expression}")


def first_argument(source, start):
    """The balanced first argument of the call whose '(' ends at start."""
    depth, quote, escaped = 0, False, False
    for index in range(start, len(source)):
        char = source[index]
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quote = False
        elif char == '"':
            quote = True
        elif char in "([{":
            depth += 1
        elif char in ")]}":
            if depth == 0:
                return source[start:index]
            depth -= 1
        elif char == "," and depth == 0:
            return source[start:index]
    raise AssertionError("Unclosed settings ID constructor")


def settings_ids(source):
    """Row and action IDs in settings_model.rs."""
    ids = set()
    for match in re.finditer(r'(?<!fn )\b(?:SurfaceSettingItem::new|action)\(\s*', source):
        expression = first_argument(source, match.end())
        if expression.strip() in ("id", "action_id"):  # the helper's own parameter
            continue
        ids.add(literal_id(expression, source))
    return ids


def plugin_ids(source):
    # Plugin rows and actions use the reviewed constructors. Their constants
    # are the IDs, so changing a prefix cannot leave a stale mapping green.
    return {value for _, value in ID_CONSTANT.findall(source)}


def launch_ids(source):
    block = source.split("pub mod launch_id {", 1)[1].split("\n}\n", 1)[0]
    ids = set(re.findall(r'pub const [A-Z_]+: &str = "([^"]*)";', block))
    ids.update(re.findall(r'format!\("([^"{]*)\{', block))
    return ids


def check_ids(ids):
    missing = ids - MAPPINGS.keys() - KNOWN_GAPS.keys()
    assert not missing, f"Settings rows need korri commands in the same change: {sorted(missing)}"


class Parity(unittest.TestCase):
    def test_settings_rows_have_cli_mappings(self):
        settings = settings_ids((SURFACE / "settings_model.rs").read_text())
        self.assertIn("device-name", settings)
        check_ids(settings)
        plugins = plugin_ids((SURFACE / "plugins.rs").read_text())
        self.assertIn("plugin-toggle:", plugins)
        check_ids(plugins)
        launch = launch_ids((SURFACE / "launch_model.rs").read_text())
        self.assertIn("launch:row:", launch)
        check_ids(launch)

    def test_surface_owned_preferences_are_recorded(self):
        for key, path in PREFERENCES.items():
            self.assertIn(f'"{key}"', (ROOT / path).read_text().replace("'", '"'))
            check_ids({key})

    def test_known_gaps_cannot_outlive_their_command(self):
        for setting, command in KNOWN_GAPS.items():
            if command is None:
                continue  # shared producer and command still unresolved
            module, verb = command
            source = (ROOT / "clients/cli/src" / module).read_text()
            self.assertNotRegex(source, rf'Command::new\(\s*"{verb}"',
                                f"{setting} now has a command; replace its known gap with a mapping")

    def test_new_row_id_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "need korri commands"):
            check_ids(settings_ids('SurfaceSettingItem::new("unmapped-setting", "Label")'))
        with self.assertRaisesRegex(AssertionError, "Unreviewed"):
            settings_ids('SurfaceSettingItem::new(NEW_SETTING, "Label")')
        self.assertEqual(
            settings_ids('const NEW_SETTING: &str = "unmapped-setting"; SurfaceSettingItem::new(NEW_SETTING, "x")'),
            {"unmapped-setting"},
        )
        # New dynamic families cannot hide behind a broad prefix rule.
        with self.assertRaisesRegex(AssertionError, "need korri commands"):
            check_ids(settings_ids('action(format!("new-family:{}", item.id), None, None)'))

    def test_new_plugin_ids_and_changed_prefixes_are_rejected(self):
        for source in [
            'const NEW_ACTION: &str = "unmapped-publisher-action";',
            'const TRUST_ACTION: &str = "unmapped-trust:";',
            'pub const PUBLISHER_ROW: &str = "unmapped-publisher:";',
        ]:
            with self.subTest(source=source), self.assertRaisesRegex(AssertionError, "need korri commands"):
                check_ids(plugin_ids(source))


if __name__ == "__main__":
    unittest.main()
