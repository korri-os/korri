#!/usr/bin/env python3
"""ADR 0031 inventory guard, not a TypeScript parser or an RPC behaviour test.

Check the existing portal row producers and ID constructors. Dynamic rows use
prefixes, as their portal controllers do. An unfamiliar expression fails closed:
when a producer changes shape, review its command instead of silently skipping it.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[3]
PORTAL = ROOT / "clients/portal/src/surface"

# Existing portal IDs, not a new product manifest. Read-only rows still name the
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
    "library-folder:": "library folders remove ID",
    "korrid-version": "status",
    "plugin-repository-add": "plugins repositories add URL",
    "plugin-repository:official": "plugins repositories list",
    "plugin-repository:": "plugins repositories list",
    "plugin-repository-remove:": "plugins repositories remove URL",
    "plugin-release:": "plugins review|install URL ID VERSION",
    "plugin-inspect:": "plugins review URL ID VERSION",
    "plugin-review:": "plugins install URL ID VERSION --approval DIGEST",
    "plugin-toggle:": "plugins on|off ID",
    "launch-device:": "launch-settings set KEY=VALUE",
    "launch:set:": "launch-settings set KEY=VALUE [layer and scope]",
    "launch:profile:pick": "launch-settings profiles use ID|--none --game GAME",
    "launch:profile:new": "launch-settings profiles new TITLE --game GAME",
    "launch:profile:remove:": "launch-settings profiles delete ID --game GAME",
    "launch:reload": "launch-settings show --game GAME",
    "launch-device:retry": "launch-settings show",
    "launch:runner": "play GAME --runner ID; launch-settings runner ID --game GAME",
    "launch:retry": "repeat the failed launch-settings command after inspection",
    "launch:cancel": "no effect; exit the editor",
}

# Rename is a temporary branch-integration gap approved by the coordinator.
# The producers branch implements it; the guard then demands a real mapping.
# Surface-owned preferences have no shared RPC or agreed command yet. Do not
# invent one to make this inventory green (see PARITY.md, schema unresolved).
KNOWN_GAPS = {
    "launch:profile:rename:": ("launch_settings.rs", "rename"),
    "pico:font": None,
    "zone:system-look": None,
}

# Launch editor row paths under launch:set:PLACE#. These are not all equivalent:
# new paths require review even though they share the launch:set prefix.
LAUNCH_PATHS = {
    '"runner"': "launch-settings runner ID",
    'options.rowPath(key)': "launch-settings set KEY=VALUE",
    '`clear:${options.rowPath(key)}`': "launch-settings set KEY --inherit",
    '"add-setting"': "launch-settings set KEY=VALUE",
    '`modifier:${modifier.id}:enabled`': "launch-settings set MODIFIER.enabled=VALUE",
    '`modifier:${modifier.id}:add-setting`': "launch-settings set MODIFIER.KEY=VALUE",
    '`file:${key}`': "launch-settings show (read-only exception)",
    '`runner-facts:${key}`': "launch-settings show (read-only facts)",
    '"runner-facts:unavailable"': "launch-settings show",
    'key': "launch-settings set cpu.min|cpu.max|gpu.min|gpu.max|cpu.governor=VALUE",
}
ROW_PATHS = {
    '`setting:${key}`': "launch-settings set KEY=VALUE",
    '`modifier:${modifier.id}:setting:${key}`': "launch-settings set MODIFIER.KEY=VALUE",
}
LIMITS = {"cpu.min", "cpu.max", "gpu.min", "gpu.max", "cpu.governor"}


def literal_id(expression, source):
    expression = expression.strip()
    if expression.startswith(('"', '`')):
        value = expression[1:-1]
        return value.split("${", 1)[0]  # dynamic ID prefix, never broad wildcard
    constant = re.search(rf'\bconst {re.escape(expression)} = ("[^"]*")', source)
    if constant:
        return literal_id(constant[1], source)
    raise AssertionError(f"Unreviewed portal setting ID expression: {expression}")


def setting_ids(source):
    ids = set()
    for match in re.finditer(r'\bid:\s*("[^"]*"|`[^`]*`|[\w$]+)', source):
        if match[1] == "string":  # function parameter type, not a settings row
            continue
        ids.add(literal_id(match[1], source))
    return ids


def launch_ids(source):
    block = source.split("export const LAUNCH_ID = {", 1)[1].split("} as const", 1)[0]
    return {literal_id(value, source) for value in re.findall(r'(?:=>\s*|:\s*)("[^"]*"|`[^`]*`)', block)}


def call_paths(source):
    """Read the balanced second argument of the existing ID constructor."""
    for match in re.finditer(r'LAUNCH_ID\.set\(placeId,\s*', source):
        depth, quote, escaped = 0, None, False
        start = match.end()
        for index in range(start, len(source)):
            char = source[index]
            if quote:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == quote:
                    quote = None
            elif char in ('"', "'", '`'):
                quote = char
            elif char == "(":
                depth += 1
            elif char == ")":
                if depth == 0:
                    yield source[start:index].strip()
                    break
                depth -= 1
        else:
            raise AssertionError("Unclosed portal ID constructor")


def check_ids(ids):
    missing = ids - MAPPINGS.keys() - KNOWN_GAPS.keys()
    assert not missing, f"Portal settings need korri commands in the same change: {sorted(missing)}"


class Parity(unittest.TestCase):
    def test_portal_settings_have_cli_mappings(self):
        source = (PORTAL / "settings-model.ts").read_text()
        check_ids(setting_ids(source))
        plugins = (PORTAL / "plugin-repositories.ts").read_text()
        # Plugin rows are built through the existing ID constructors. Include
        # their literal constants plus literal row IDs added in future changes.
        factories = {"toggleSettingId": "plugin-toggle:", "releaseRowId": "plugin-release:",
                     "repositoryRowId": "plugin-repository:", "removeRepositoryActionId": "plugin-repository-remove:"}
        for value in re.findall(r'\bid:\s*("[^"]*"|`[^`]*`|[\w$]+)', plugins):
            if value in ("string", '`${repositoryRowId(url)}:status`'):
                continue  # parameter type or read-only catalog status
            check_ids({factories[value] if value in factories else literal_id(value, plugins)})
        # Also read the constants behind those factories, so changing a dynamic
        # family cannot keep passing through a stale factory-name mapping.
        check_ids({literal_id(value, plugins) for value in re.findall(
            r'const (?:REPOSITORY_ROW|REMOVE_ACTION|RELEASE_ROW|INSPECT_ACTION|REVIEW_ACTION|TOGGLE_SETTING) = ("[^"]*")', plugins)})
        launch = (PORTAL / "launch-settings-model.ts").read_text()
        check_ids(launch_ids(launch))
        # Explicit constructor arguments: any new path fails, including a path
        # whose ID uses the already-mapped launch:set prefix.
        paths = set(call_paths(launch))
        self.assertTrue(paths)
        self.assertFalse(paths - LAUNCH_PATHS.keys(), paths - LAUNCH_PATHS.keys())
        row_paths = set(re.findall(r'rowPath: key => (`[^`]*`)', launch))
        self.assertEqual(row_paths, ROW_PATHS.keys())
        limits = launch.split("export type LimitKey = ", 1)[1].split("\n", 1)[0]
        self.assertEqual(set(re.findall(r'"([^"]*)"', limits)), LIMITS)

    def test_surface_owned_preferences_are_recorded(self):
        for path, constant in [
            ("surfaces/pico/src/pico-settings-view.ts", "PICO_FONT_SETTING"),
            ("surfaces/zone/src/plan/options.ts", "ZONE_SYSTEM_LOOK"),
        ]:
            source = (ROOT / path).read_text()
            check_ids({literal_id(constant, source)})

    def test_known_gaps_cannot_outlive_their_command(self):
        for setting, command in KNOWN_GAPS.items():
            if command is None:
                continue  # shared producer and command still unresolved
            module, verb = command
            source = (ROOT / "clients/cli/src" / module).read_text()
            self.assertNotRegex(source, rf'Command::new\(\s*"{verb}"',
                                f"{setting} now has a command; replace its known gap with a mapping")

    def test_new_portal_id_is_rejected(self):
        with self.assertRaisesRegex(AssertionError, "need korri commands"):
            check_ids(setting_ids('{ id: "unmapped-setting", interaction: { kind: "text" } }'))
        with self.assertRaisesRegex(AssertionError, "Unreviewed"):
            setting_ids('{ id: NEW_SETTING, interaction: { kind: "text" } }')
        self.assertEqual(setting_ids('const NEW_SETTING = "unmapped-setting"; { id: NEW_SETTING }'), {"unmapped-setting"})
        # New dynamic families cannot hide behind a broad prefix rule.
        with self.assertRaisesRegex(AssertionError, "need korri commands"):
            check_ids(setting_ids('{ id: `new-family:${item.id}` }'))


if __name__ == "__main__":
    unittest.main()
