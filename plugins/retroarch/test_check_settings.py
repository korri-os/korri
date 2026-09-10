import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("check_settings", Path(__file__).with_name("check-settings.py"))
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class SourceSettingsTests(unittest.TestCase):
    def test_key_and_type_must_both_match(self):
        accepted, omitted = checker.check(
            {"video_vsync": "Boolean", "audio_volume": "Number", "audio_device": "String",
             "absent": "Boolean", "changed_type": "Boolean"},
            '''SETTING_BOOL("video_vsync", &value, true, true, true);
               SETTING_FLOAT("audio_volume", &value, true, 0, true);
               SETTING_ARRAY("audio_device", value, false, NULL, true);
               SETTING_UINT("changed_type", &value, true, 0, true);''',
            set(),
        )
        self.assertEqual(accepted, {"video_vsync": "Boolean", "audio_volume": "Number", "audio_device": "String"})
        self.assertEqual(set(omitted), {"absent", "changed_type"})

    def test_reserved_commented_and_conflicting_keys_are_not_evidence(self):
        accepted, omitted = checker.check(
            {key: "Boolean" for key in ["reserved", "commented", "conflict", "direct"]},
            '''SETTING_BOOL("reserved", &value, true, true, true);
               /* SETTING_BOOL("commented", &value, true, true, true); */
               SETTING_BOOL("conflict", &value, true, true, true);
               SETTING_INT("conflict", &value, true, 0, true);
               config_get_bool(conf, "direct", &value);''',
            {"reserved"},
        )
        self.assertEqual(accepted, {"direct": "Boolean"})
        self.assertEqual(set(omitted), {"reserved", "commented", "conflict"})

    def test_each_instance_source_is_checked_independently(self):
        table = {"video_vsync": "Boolean"}
        first, _ = checker.check(table, 'SETTING_BOOL("video_vsync", &v, true, true, true);', set())
        second, _ = checker.check(table, 'SETTING_UINT("video_vsync", &v, true, 1, true);', set())
        missing, _ = checker.check(table, '', set())
        self.assertEqual(first, table)
        self.assertEqual(second, {})
        self.assertEqual(missing, {})

    def test_native_read_macros_carry_types(self):
        self.assertEqual(checker.source_types('''
            CONFIG_GET_INT_BASE(conf, settings, uints.value, "number");
            config_get_path(conf, "path", value, sizeof(value));
            // SETTING_BOOL("comment", &value, true, true, true);
        '''), {"number": "Number", "path": "String"})


if __name__ == "__main__":
    unittest.main()
