#!/usr/bin/env nix
#! nix shell nixpkgs#python3 nixpkgs#python3Packages.pyyaml --command python3
"""Offline workflow and CLI orchestration tests; no network or package builds."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from urllib.parse import parse_qs, urlsplit


REPOSITORY = "owner/repository"
ROOT = f"repos/{REPOSITORY}"
TAG = "plugins-2"
REVISION = "a" * 40
CREATED_ID = 101
RELEASE_ENDPOINT = f"{ROOT}/releases/{CREATED_ID}"
UPLOAD_URL = f"https://uploads.github.com/{RELEASE_ENDPOINT}/assets"


def release_record(identity, tag=TAG, draft=True):
    endpoint = f"{ROOT}/releases/{identity}"
    return {
        "id": identity,
        "tag_name": tag,
        "draft": draft,
        "url": f"https://api.github.com/{endpoint}",
        "upload_url": f"https://uploads.github.com/{endpoint}/assets{{?name,label}}",
    }


def controlled_process(program):
    """A stateful CLI implementation with real files, uploads and read failures.

    Only the documented REST operations used by preparation are implemented.
    An unexpected command fails instead of delegating to an installed CLI.
    The verifier checks real file bytes; Rust's catalog contract has its own tests.
    """
    state_path = Path(os.environ["PUBLICATION_TEST_STATE"])
    state = json.loads(state_path.read_text())

    def save():
        state_path.write_text(json.dumps(state))

    def fail(message):
        save()
        sys.exit(message)

    if program == "korri-publish":
        assert sys.argv[1] == "verify-release"
        directory = Path(sys.argv[3])
        state["calls"].append({"program": program, "directory": str(directory)})
        save()
        expected = state["expected_assets"]
        if set(path.name for path in directory.iterdir()) != set(expected) or any(
            (directory / name).read_bytes().hex() != data
            for name, data in expected.items()
        ):
            fail("downloaded bytes failed verification")
        print("\n".join(expected))
        return
    if program != "gh":
        fail("a write-token step must not invoke Nix")

    parser = argparse.ArgumentParser()
    parser.add_argument("api", choices=["api"])
    parser.add_argument("--hostname", choices=["github.com"], required=True)
    parser.add_argument("--method", choices=["GET", "POST"], required=True)
    parser.add_argument("--header", action="append", required=True)
    parser.add_argument("--input")
    parser.add_argument("endpoint")
    args = parser.parse_args()
    call = {"program": program, "method": args.method, "endpoint": args.endpoint}
    state["calls"].append(call)
    save()
    if args.method == "GET" and args.endpoint == state.get("fail_read"):
        fail(state.get("read_error", "HTTP 401: Bad credentials"))

    parsed = urlsplit(args.endpoint)
    query = parse_qs(parsed.query)
    response = None
    if args.method == "GET" and parsed.path == ROOT + "/commits/refs%2Ftags%2F" + TAG:
        response = {"sha": REVISION}
    elif args.method == "GET" and parsed.path == ROOT + "/releases":
        assert query["per_page"] == ["100"]
        page = int(query["page"][0])
        response = state["releases"][(page - 1) * 100 : page * 100]
        if "list_response" in state:
            response = state["list_response"]
    elif args.method == "POST" and args.endpoint == ROOT + "/releases":
        assert args.input == "-"
        call["body"] = json.load(sys.stdin)
        assert call["body"]["tag_name"] == TAG
        assert call["body"]["draft"] is True
        assert set(call["body"]) == {"tag_name", "draft", "name", "body"}
        # Deliberately allow duplicate tags, as GitHub does for draft creation.
        response = release_record(CREATED_ID)
        state["releases"].append(response.copy())
        if state.get("ambiguity_after_create"):
            state["releases"].append(release_record(202, draft=False))
        if state.get("publish_before_upload"):
            state["releases"][-1]["draft"] = False
        response.update(state.get("creation_overrides", {}))
    elif args.method == "GET" and args.endpoint == RELEASE_ENDPOINT:
        response = next(
            record for record in state["releases"] if record["id"] == CREATED_ID
        )
    elif args.method == "GET" and parsed.path == RELEASE_ENDPOINT + "/assets":
        assert query["per_page"] == ["100"]
        page = int(query["page"][0])
        response = state["assets"][(page - 1) * 100 : page * 100]
    elif args.method == "POST" and args.endpoint.split("?")[0] == UPLOAD_URL:
        assert "Content-Type: application/octet-stream" in args.header
        assert next(
            record for record in state["releases"] if record["id"] == CREATED_ID
        )["draft"]
        (name,) = query["name"]
        call["name"] = name
        if any(asset["name"] == name for asset in state["assets"]):
            fail("HTTP 422: asset name already exists")
        if state.get("upload_failure") == len(state["assets"]) + 1:
            fail("HTTP 502: upload failed")
        identity = 1001 + len(state["assets"])
        response = {
            "id": identity,
            "name": name,
            "state": "uploaded",
            "url": f"https://api.github.com/{ROOT}/releases/assets/{identity}",
        }
        state["assets"].append(response.copy())
        state["asset_bytes"][str(identity)] = Path(args.input).read_bytes().hex()
        if state.get("ambiguity_after_upload"):
            state["releases"].append(release_record(202, draft=False))
        if state.get("publish_after_upload"):
            next(record for record in state["releases"] if record["id"] == CREATED_ID)[
                "draft"
            ] = False
        response.update(state.get("upload_overrides", {}))
    elif args.method == "GET" and parsed.path.startswith(ROOT + "/releases/assets/"):
        assert "Accept: application/octet-stream" in args.header
        identity = parsed.path.rsplit("/", 1)[1]
        data = bytes.fromhex(state["asset_bytes"][identity])
        if state.get("replace_after_download"):
            record = state["assets"][0]
            record["id"] = 9001
            record["url"] = f"https://api.github.com/{ROOT}/releases/assets/9001"
        save()
        sys.stdout.buffer.write(
            b"corrupt bytes" if state.get("corrupt_download") else data
        )
        return
    else:
        fail(f"unexpected API request: {args.method} {args.endpoint}")
    save()
    print(json.dumps(response))


if Path(sys.argv[0]).name in ("gh", "korri-publish", "nix"):
    controlled_process(Path(sys.argv[0]).name)
    sys.exit(0)

SCRIPT = Path(sys.argv.pop(1)).resolve()
WORKFLOW = Path(sys.argv.pop(1)).resolve()
spec = importlib.util.spec_from_file_location("publication", SCRIPT)
publication = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publication)


class PublicationTests(unittest.TestCase):
    def env(self):
        return {
            **os.environ,
            "GITHUB_REPOSITORY_OWNER": "owner",
            "GITHUB_REPOSITORY": REPOSITORY,
            "GITHUB_ACTOR": "owner",
            "GITHUB_TRIGGERING_ACTOR": "owner",
            "GITHUB_EVENT_NAME": "workflow_dispatch",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_SHA": REVISION,
            "PLUGIN_DESTINATION": REPOSITORY,
            "PLUGIN_TAG": TAG,
            "PLUGIN_RELEASE": "2",
        }

    def invoke(self, *args, **changes):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            env={**self.env(), **changes},
            capture_output=True,
        )

    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.output = self.root / "prepared"
        (self.output / "assets").mkdir(parents=True)
        (self.output / "catalog.json").write_text("{}")
        self.initial = {
            "releases": [],
            "assets": [],
            "asset_bytes": {},
            "calls": [],
            "expected_assets": {
                "one.tar": b"archive one\x00".hex(),
                "two.tar": b"archive two\xff".hex(),
            },
        }
        for name, content in self.initial["expected_assets"].items():
            (self.output / "assets" / name).write_bytes(bytes.fromhex(content))
        self.state_path = self.root / "process-state.json"
        self.state_path.write_text(json.dumps(self.initial))
        self.bin = self.root / "bin"
        self.bin.mkdir()
        # These subprocess fixtures use the test runner's interpreter, including
        # inside a Nix sandbox where /usr/bin/env and nested Nix are unavailable.
        source = f"#!{sys.executable}\n" + Path(__file__).read_text()
        for name in ("gh", "korri-publish", "nix"):
            command = self.bin / name
            command.write_text(source)
            command.chmod(0o700)

    def state(self):
        return json.loads(self.state_path.read_text())

    def draft(self, **configuration):
        state = self.state()
        state.update(configuration)
        self.state_path.write_text(json.dumps(state))
        return self.invoke(
            "draft",
            str(self.output),
            PLUGIN_CREATE_DRAFT="true",
            KORRI_PUBLICATION_PUBLISHER=str(self.bin / "korri-publish"),
            PUBLICATION_TEST_STATE=str(self.state_path),
            PATH=str(self.bin),
            GH_TOKEN="controlled-test-token",
            GH_HOST="untrusted.invalid",
        )

    def writes(self):
        return [call for call in self.state()["calls"] if call.get("method") == "POST"]

    def test_only_owner_main_dispatch_and_owner_rerun_pass(self):
        self.assertEqual(self.invoke("guard").returncode, 0)
        for field, value in [
            ("GITHUB_EVENT_NAME", "pull_request"),
            ("GITHUB_EVENT_NAME", "push"),
            ("GITHUB_EVENT_NAME", "pull_request_target"),
            ("GITHUB_REF", "refs/heads/contributor"),
            ("GITHUB_ACTOR", "contributor"),
            ("GITHUB_TRIGGERING_ACTOR", "contributor"),
            ("PLUGIN_DESTINATION", ""),
            ("PLUGIN_DESTINATION", "https://github.com/owner/repo"),
            ("PLUGIN_DESTINATION", "../repo"),
            ("PLUGIN_TAG", "../other"),
            ("PLUGIN_RELEASE", ""),
        ]:
            with self.subTest(field=field, value=value):
                self.assertNotEqual(
                    self.invoke("guard", **{field: value}).returncode, 0
                )

    def test_draft_requires_explicit_approval_and_current_repository(self):
        result = self.invoke(
            "draft",
            "absent",
            KORRI_PUBLICATION_PUBLISHER="absent",
            PLUGIN_CREATE_DRAFT="false",
        )
        self.assertIn(b"explicit operator approval", result.stderr)
        result = self.invoke(
            "draft",
            "absent",
            KORRI_PUBLICATION_PUBLISHER="absent",
            PLUGIN_CREATE_DRAFT="true",
            PLUGIN_DESTINATION="other/repository",
        )
        self.assertIn(b"limited to this repository", result.stderr)

    def test_existing_draft_or_published_release_on_any_page_blocks_all_writes(self):
        for draft in (True, False):
            for preceding in (0, 100):
                with self.subTest(draft=draft, preceding=preceding):
                    records = [
                        release_record(300 + i, tag=f"unrelated-{i}")
                        for i in range(preceding)
                    ]
                    records.append(release_record(7, draft=draft))
                    # A different asset name must not make a published release reusable.
                    result = self.draft(
                        releases=records, calls=[], assets=[{"name": "old-label.tar"}]
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn(b"tag already has", result.stderr)
                    self.assertEqual(self.writes(), [])
                    self.assertEqual(self.state()["releases"], records)
                    self.assertEqual(
                        self.state()["assets"], [{"name": "old-label.tar"}]
                    )
                    if preceding:
                        self.assertIn(
                            ROOT + "/releases?per_page=100&page=2",
                            [call.get("endpoint") for call in self.state()["calls"]],
                        )

    def test_read_auth_and_network_errors_are_not_absence_even_on_later_pages(self):
        for endpoint, error in [
            (ROOT + "/commits/refs%2Ftags%2F" + TAG, "HTTP 404: tag not found"),
            (ROOT + "/releases?per_page=100&page=1", "HTTP 401: Bad credentials"),
            (ROOT + "/releases?per_page=100&page=2", "connection reset"),
        ]:
            with self.subTest(endpoint=endpoint):
                result = self.draft(
                    fail_read=endpoint,
                    read_error=error,
                    calls=[],
                    releases=[
                        release_record(300 + i, tag=f"other-{i}") for i in range(100)
                    ],
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error.encode(), result.stderr)
                self.assertEqual(self.writes(), [])

    def test_malformed_list_is_not_absence(self):
        for response in (
            {},
            [None],
            [{"id": True, "tag_name": "other", "draft": True}],
            [{"id": 1, "tag_name": None, "draft": True}],
        ):
            with self.subTest(response=response):
                self.assertNotEqual(
                    self.draft(list_response=response, calls=[]).returncode, 0
                )
                self.assertEqual(self.writes(), [])

    def test_tag_ambiguity_after_creation_stops_before_upload(self):
        result = self.draft(ambiguity_after_create=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"tag already has", result.stderr)
        self.assertEqual(
            [call["endpoint"] for call in self.writes()], [ROOT + "/releases"]
        )
        self.assertTrue(self.state()["releases"][0]["draft"])
        self.assertEqual(self.state()["assets"], [])

    def test_tag_ambiguity_during_upload_never_retargets_a_write(self):
        result = self.draft(ambiguity_after_upload=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(
            [call["endpoint"] for call in self.writes()],
            [ROOT + "/releases", UPLOAD_URL + "?name=one.tar"],
        )
        self.assertEqual(
            self.state()["releases"],
            [release_record(CREATED_ID), release_record(202, draft=False)],
        )

    def test_exact_release_and_asset_ids_upload_download_and_verify_real_bytes(self):
        # Exercise successful traversal beyond the first page, too.
        result = self.draft(
            releases=[release_record(300 + i, tag=f"other-{i}") for i in range(100)]
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertIn(b"Draft 101 assets verified", result.stdout)
        writes = self.writes()
        self.assertEqual(
            [call["endpoint"] for call in writes],
            [
                ROOT + "/releases",
                UPLOAD_URL + "?name=one.tar",
                UPLOAD_URL + "?name=two.tar",
            ],
        )
        self.assertIs(writes[0]["body"]["draft"], True)
        calls = self.state()["calls"]
        self.assertEqual(
            [
                call["endpoint"]
                for call in calls
                if call.get("endpoint", "").startswith(ROOT + "/releases/assets/")
            ],
            [ROOT + "/releases/assets/1001", ROOT + "/releases/assets/1002"],
        )
        verifications = [
            call["directory"] for call in calls if call["program"] == "korri-publish"
        ]
        self.assertEqual(len(verifications), 2)
        self.assertEqual(verifications[0], str(self.output / "assets"))
        self.assertNotEqual(verifications[1], verifications[0])
        self.assertEqual(
            self.state()["asset_bytes"],
            {
                "1001": self.initial["expected_assets"]["one.tar"],
                "1002": self.initial["expected_assets"]["two.tar"],
            },
        )
        for write in writes[1:]:
            index = calls.index(write)
            self.assertEqual(
                calls[index - 1],
                {"program": "gh", "method": "GET", "endpoint": RELEASE_ENDPOINT},
            )
            self.assertIn(
                {"program": "gh", "method": "GET", "endpoint": RELEASE_ENDPOINT},
                calls[index + 1 :],
            )
        self.assertEqual(
            calls[-1], {"program": "gh", "method": "GET", "endpoint": RELEASE_ENDPOINT}
        )

    def test_partial_upload_failure_leaves_draft_and_retry_does_not_overwrite(self):
        result = self.draft(upload_failure=2)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.state()["releases"], [release_record(CREATED_ID)])
        self.assertEqual(
            [asset["name"] for asset in self.state()["assets"]], ["one.tar"]
        )
        before = self.state()
        retry = self.draft(upload_failure=0, calls=[])
        self.assertNotEqual(retry.returncode, 0)
        self.assertEqual(self.writes(), [])
        self.assertEqual(self.state()["assets"], before["assets"])
        self.assertEqual(self.state()["asset_bytes"], before["asset_bytes"])
        self.assertEqual(self.state()["releases"], before["releases"])

    def test_untrusted_creation_urls_and_invalid_ids_fail_before_asset_writes(self):
        for override in [
            {
                "upload_url": UPLOAD_URL.replace("uploads.github.com", "evil.invalid")
                + "{?name,label}"
            },
            {"upload_url": UPLOAD_URL.replace("https:", "http:") + "{?name,label}"},
            {
                "upload_url": UPLOAD_URL.replace(
                    "uploads.github.com", "uploads.github.com@evil.invalid"
                )
                + "{?name,label}"
            },
            {"upload_url": UPLOAD_URL.replace("/101/", "/202/") + "{?name,label}"},
            {"url": "https://evil.invalid/release"},
            {"id": "../202"},
            {"id": True},
            {"id": 0},
            {"tag_name": "other"},
            {"draft": False},
        ]:
            with self.subTest(override=override):
                result = self.draft(creation_overrides=override, releases=[], calls=[])
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(
                    [call["endpoint"] for call in self.writes()], [ROOT + "/releases"]
                )

    def test_invalid_upload_identity_is_never_used_for_download(self):
        for override in (
            {"id": "../202"},
            {"url": "https://evil.invalid/asset"},
            {"name": "../escape.tar"},
            {"state": "starter"},
        ):
            with self.subTest(override=override):
                result = self.draft(
                    upload_overrides=override, releases=[], assets=[], calls=[]
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(len(self.writes()), 2)
                self.assertFalse(
                    any(
                        call.get("endpoint", "").startswith(ROOT + "/releases/assets/")
                        for call in self.state()["calls"]
                    )
                )

    def test_draft_is_reread_before_the_first_upload(self):
        result = self.draft(publish_before_upload=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"not a draft", result.stderr)
        self.assertEqual(len(self.writes()), 1)

    def test_publishing_during_a_write_is_detected_but_not_a_transaction(self):
        result = self.draft(publish_after_upload=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"not a draft", result.stderr)
        self.assertEqual(len(self.writes()), 2)
        self.assertNotIn(b"assets verified", result.stdout)

    def test_corrupt_download_fails_and_leaves_the_draft_untouched(self):
        result = self.draft(corrupt_download=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"downloaded bytes failed verification", result.stderr)
        self.assertEqual(self.state()["releases"], [release_record(CREATED_ID)])
        self.assertEqual(len(self.writes()), 3)

    def test_asset_replacement_during_verification_fails_even_with_same_bytes(self):
        result = self.draft(replace_after_download=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b"replaced assets", result.stderr)
        self.assertEqual(self.state()["releases"], [release_record(CREATED_ID)])
        self.assertEqual(len(self.writes()), 3)

    def test_workflow_scope_permissions_pins_and_dependency_gates(self):
        import yaml

        # BaseLoader uses YAML 1.2-compatible string keys for GitHub's `on`.
        workflow = yaml.load(WORKFLOW.read_text(), Loader=yaml.BaseLoader)
        self.assertEqual(set(workflow["on"]), {"workflow_dispatch"})
        self.assertEqual(workflow["permissions"], {"contents": "read"})
        jobs = workflow["jobs"]
        matrix = jobs["build"]["strategy"]["matrix"]["include"]
        self.assertEqual(
            matrix,
            [
                {"system": "x86_64-linux", "runner": "ubuntu-24.04"},
                {"system": "aarch64-linux", "runner": "ubuntu-24.04-arm"},
            ],
        )
        self.assertEqual(tuple(item["system"] for item in matrix), publication.SYSTEMS)
        self.assertEqual(jobs["build"]["needs"], "guard")
        self.assertEqual(jobs["lifecycle"]["needs"], "guard")
        self.assertEqual(jobs["assemble"]["needs"], ["build", "lifecycle"])
        self.assertEqual(jobs["draft"]["needs"], "assemble")
        self.assertEqual(jobs["draft"]["environment"], "plugin-release")
        for job_name, job in jobs.items():
            if job_name == "draft":
                self.assertEqual(job["permissions"], {"contents": "write"})
                self.assertIn("inputs.create_draft", job["if"])
                self.assertNotIn("GH_TOKEN", job.get("env", {}))
            else:
                self.assertNotIn("permissions", job)
            if job_name in ("guard", "draft"):
                for guard in (
                    "github.event_name == 'workflow_dispatch'",
                    "github.ref == 'refs/heads/main'",
                    "github.actor == github.repository_owner",
                    "github.triggering_actor == github.repository_owner",
                ):
                    self.assertIn(guard, job["if"])
            for step in job["steps"]:
                if "uses" in step:
                    self.assertRegex(step["uses"], r"^[\w/-]+@[a-f0-9]{40}$")
                if step.get("uses", "").startswith("actions/checkout@"):
                    self.assertEqual(step["with"]["persist-credentials"], "false")
                if "GH_TOKEN" in step.get("env", {}):
                    self.assertEqual(job_name, "draft")
                    self.assertIn("KORRI_PUBLICATION_PUBLISHER", step["env"])
                    self.assertNotIn("nix ", step.get("run", ""))
        self.assertNotIn("pages: write", WORKFLOW.read_text())
        self.assertNotIn("--clobber", WORKFLOW.read_text())


if __name__ == "__main__":
    unittest.main()
