#!/usr/bin/env python3
"""Validate and compare the single explicitly registered Oracle #685 known-red."""

import base64
import hashlib
import json
import pathlib
import re
import sys

RECORD_REL = pathlib.Path("scripts/guards/novac-known-red-685.json")
FIXTURE_REL = "novac/fixtures/call_operands/pos_1.nv"
REGISTRY_REL = "docs/plans/221.1-bug-sweep.md"
FIXTURE_NORMALIZATION = "crlf-to-lf"
OBSERVATION_KEYS = {"exit", "stdout_b64", "stderr_b64"}


def fail(message):
    raise ValueError(message)


def no_duplicate_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            fail("duplicate JSON key: " + key)
        result[key] = value
    return result


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=no_duplicate_keys)
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        fail("cannot parse " + str(path) + ": " + str(error))


def observation(value, where):
    if not isinstance(value, dict) or set(value) != OBSERVATION_KEYS:
        fail(where + " must contain exactly exit/stdout_b64/stderr_b64")
    if type(value["exit"]) is not int or not 0 <= value["exit"] <= 255:
        fail(where + " has invalid exit status")
    decoded = {}
    for field in ("stdout_b64", "stderr_b64"):
        try:
            decoded[field] = base64.b64decode(value[field], validate=True)
        except (TypeError, ValueError):
            fail(where + " has invalid " + field)
        if base64.b64encode(decoded[field]).decode("ascii") != value[field]:
            fail(where + " has non-canonical " + field)
    return {"exit": value["exit"], "stdout": decoded["stdout_b64"], "stderr": decoded["stderr_b64"]}


def load_record(root):
    path = root / RECORD_REL
    if not path.exists():
        return None
    data = read_json(path)
    if (not isinstance(data, dict) or set(data) != {"schema", "entries"}
            or type(data["schema"]) is not int or data["schema"] != 1):
        fail("record must have schema=1 and entries only")
    entries = data["entries"]
    if not isinstance(entries, list) or len(entries) != 1:
        fail("record must contain exactly one KNOWN-RED entry (№685)")
    entry = entries[0]
    fields = {"issue", "fixture", "registry", "fixture_normalization", "fixture_sha256", "platforms"}
    if not isinstance(entry, dict) or set(entry) != fields:
        fail("KNOWN-RED entry has missing or unknown fields")
    if entry["issue"] != 685 or entry["fixture"] != FIXTURE_REL or entry["registry"] != REGISTRY_REL:
        fail("only №685 / " + FIXTURE_REL + " / " + REGISTRY_REL + " is permitted")
    if entry["fixture_normalization"] != FIXTURE_NORMALIZATION:
        fail("fixture_normalization must be exactly " + FIXTURE_NORMALIZATION)
    registry = root / REGISTRY_REL
    if not registry.is_file():
        fail("missing registry link target: " + REGISTRY_REL)
    try:
        registry_lines = registry.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        fail("cannot read registry target: " + str(error))
    if not any(re.match(r"^\|\s*685\s*\|", line) for line in registry_lines):
        fail("registry target has no №685 row: " + REGISTRY_REL)
    fixture = root / FIXTURE_REL
    if not fixture.is_file():
        fail("missing known-red fixture: " + FIXTURE_REL)
    canonical_fixture = fixture.read_bytes().replace(b"\r\n", b"\n")
    fixture_hash = hashlib.sha256(canonical_fixture).hexdigest()
    pinned_fixture_hash = entry["fixture_sha256"]
    if (not isinstance(pinned_fixture_hash, str) or len(pinned_fixture_hash) != 64
            or any(character not in "0123456789abcdef" for character in pinned_fixture_hash)):
        fail("fixture_sha256 must be 64 lowercase hexadecimal characters")
    if fixture_hash != pinned_fixture_hash:
        fail("known-red fixture changed: expected SHA-256 " + pinned_fixture_hash + ", got " + fixture_hash)
    platforms = entry["platforms"]
    if not isinstance(platforms, dict) or set(platforms) != {"windows"}:
        fail("the only recorded KNOWN-RED signature must be Windows-specific")
    decoded = {}
    for platform in ("windows",):
        pair = platforms[platform]
        if not isinstance(pair, dict) or set(pair) != {"oracle", "carina"}:
            fail(platform + " signature must contain exactly oracle and carina")
        decoded[platform] = {
            compiler: observation(pair[compiler], platform + "/" + compiler)
            for compiler in ("oracle", "carina")
        }
    if decoded["windows"]["oracle"] == decoded["windows"]["carina"]:
        fail("Windows entry is not a measured divergence")
    return entry


def current_platform():
    if sys.platform == "win32":
        return "windows"
    if sys.platform.startswith("linux"):
        return "linux"
    fail("unsupported known-red platform: " + sys.platform)


def report_line(entry, platform, observations):
    def side(name):
        item = observations[name]
        stdout = base64.b64decode(item["stdout_b64"])
        stderr = base64.b64decode(item["stderr_b64"])
        return ("%s(exit=%d,stdout-sha256=%s,stderr-sha256=%s)" % (
            name, item["exit"], hashlib.sha256(stdout).hexdigest(), hashlib.sha256(stderr).hexdigest()))

    return ("check-novac-differential: KNOWN-RED №685 path=%s registry=%s platform=%s; "
            "%s vs %s; recognized Oracle defect, not language conformance" % (
                entry["fixture"], entry["registry"], platform, side("oracle"), side("carina")))


def main(argv):
    if len(argv) == 3 and argv[1] == "validate":
        entry = load_record(pathlib.Path(argv[2]).resolve())
        print("absent" if entry is None else "present")
        return 0
    if len(argv) == 4 and argv[1] == "compare":
        root = pathlib.Path(argv[2]).resolve()
        entry = load_record(root)
        if entry is None:
            fail("known-red record is absent")
        signature_path = pathlib.Path(argv[3])
        actual = read_json(signature_path)
        if not isinstance(actual, dict) or set(actual) != {"oracle", "carina"}:
            fail("observation sidecar must contain exactly oracle and carina")
        platform = current_platform()
        actual_decoded = {
            compiler: observation(actual[compiler], "observed/" + compiler)
            for compiler in ("oracle", "carina")
        }
        if platform == "linux":
            if actual_decoded["oracle"] != actual_decoded["carina"]:
                fail("Linux ordinary differential: Oracle and Carina stdout, stderr, or exit differ")
            print("pos_1 Oracle and Carina observations match on Linux; ordinary differential, no KNOWN-RED")
            return 0
        expected = entry["platforms"]["windows"]
        expected_decoded = {
            compiler: observation(expected[compiler], "windows/" + compiler)
            for compiler in ("oracle", "carina")
        }
        if actual_decoded != expected_decoded:
            fail("observed Oracle/Carina stdout, stderr, or exit differs from pinned Windows signature")
        if actual_decoded["oracle"] != actual_decoded["carina"]:
            print(report_line(entry, platform, actual))
            print("KNOWN-RED 1 (excluded from behavior-match count; not spec-conformance PASS)")
        else:
            fail("Windows Oracle and Carina observations unexpectedly converge; remove the fixed KNOWN-RED")
        return 0
    fail("usage: novac-known-red-685.py validate ROOT | compare ROOT SIGNATURE.json")


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except (ValueError, KeyError, TypeError) as error:
        print("check-novac-differential: FAIL — known-red №685: " + str(error), file=sys.stderr)
        sys.exit(1)
