#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Check immutable-plugin read scope and fail-closed diagnostic handling."""

import importlib.util
from pathlib import Path
import sys
import unittest

SOURCE = Path(sys.argv.pop(1)) if len(sys.argv) > 1 else Path(__file__).with_name("plugin-read-failure.py")
SPEC = importlib.util.spec_from_file_location("plugin_read_failure", SOURCE)
reads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reads)
CACHE = "https://cache.test/"
PIN = "/nix/store/" + "a" * 32 + "-korri-plugin"
OTHER = "/nix/store/" + "b" * 32 + "-korri-plugin"
EVAL = ["eval", "--json", ".#nixosConfigurations.rpminiv2.config.sdImage.storePaths", "--apply", "xs: map toString xs"]
INFO = ["path-info", "--store", CACHE, "--json", PIN]
BUILD = ["build", "--no-link", PIN, "--option", "max-jobs", "0", "--option", "builders", "", "--option", "fallback", "false", "--option", "require-sigs", "true"]
MISS = f"error: path '{PIN}' is required, but there is no substituter that can build it\n"
DOWNLOAD = f"error: unable to download '{CACHE}{'a' * 32}.narinfo': HTTP error 404\n"


class PluginReadScopeTest(unittest.TestCase):
    def allowed(self, text, command=EVAL):
        return reads.transient_plugin_read(text, CACHE, [PIN], command)

    def test_bare_missing_pin_is_scoped_to_actual_read_commands(self):
        self.assertTrue(self.allowed(MISS))
        self.assertTrue(self.allowed(MISS, BUILD))
        self.assertTrue(self.allowed(f"error: path '{PIN}' is not valid\n", INFO))
        for command in ([], ["eval", "--expr", "arbitrary"], ["build", PIN], INFO[:]):
            text = MISS.replace(PIN, OTHER) if command == INFO else MISS
            with self.subTest(command=command):
                self.assertFalse(self.allowed(text, command))
        self.assertFalse(self.allowed(MISS.replace(PIN, OTHER)))

    def test_successful_null_metadata_is_only_a_scoped_remote_read(self):
        self.assertTrue(reads.transient_plugin_read("", CACHE, [PIN], INFO, missing_metadata=True))
        self.assertFalse(reads.transient_plugin_read("", CACHE, [PIN], EVAL, missing_metadata=True))
        self.assertFalse(reads.transient_plugin_read("", CACHE, [OTHER], INFO, missing_metadata=True))
        self.assertFalse(reads.transient_plugin_read(
            "warning: not signed by a trusted key\n", CACHE, [PIN], INFO, missing_metadata=True
        ))

    def test_transient_narinfo_is_correlated_with_requested_pin(self):
        self.assertTrue(self.allowed(DOWNLOAD + MISS))
        self.assertFalse(self.allowed(DOWNLOAD.replace("a" * 32, "b" * 32) + MISS))
        self.assertFalse(self.allowed(DOWNLOAD.replace(".narinfo", ".json") + MISS))

    def test_payload_miss_requires_matching_real_copy_origin(self):
        copy = f"copying path '{PIN}' from '{CACHE.rstrip('/')}'...\n"
        missing = f"warning: file 'nar/payload.nar.xz' does not exist in binary cache '{CACHE}'\n"
        self.assertTrue(self.allowed(copy + missing + MISS, BUILD))
        self.assertFalse(self.allowed(missing + MISS, BUILD))
        self.assertFalse(self.allowed(copy + missing.replace(CACHE, "https://other.test/") + MISS, BUILD))

    def test_unknown_and_warning_integrity_metadata_errors_veto_every_read(self):
        for diagnostic in (
            "warning: ignoring substitute as it's not signed by any trusted key\n",
            "warning: NAR hash mismatch\n",
            "warning: invalid metadata\n",
            "error: assertion failed\n",
            "error[E0308]: type mismatch\n",
            "error: builder for fixture failed\n",
            "error: unexpected failure\n",
        ):
            for read in (MISS, DOWNLOAD + MISS):
                with self.subTest(diagnostic=diagnostic, read=read):
                    self.assertFalse(self.allowed(diagnostic + read))

    def test_context_frames_and_known_nix_deprecation_do_not_hide_cache_miss(self):
        self.assertTrue(self.allowed(
            "error:\n  … while calling appendContext\n"
            "warning: '--json' without '--json-format' is deprecated; use a format\n" + MISS
        ))

    def test_unknown_failures_never_retry(self):
        self.assertFalse(self.allowed("error: unrelated\n"))
        self.assertFalse(self.allowed(""))
        self.assertFalse(self.allowed(DOWNLOAD.replace("404", "401") + MISS))
        self.assertFalse(self.allowed(DOWNLOAD.replace(CACHE, "https://[invalid/") + MISS))


if __name__ == "__main__":
    unittest.main()
