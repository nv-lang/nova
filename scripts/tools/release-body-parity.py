#!/usr/bin/env python3
"""Compare a GitHub release body with its checked-in Markdown source."""

from __future__ import annotations

import argparse
import contextlib
import difflib
import io
import json
import os
import posixpath
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path


LINK = re.compile(r"(?<!!)\[([^\]]*)\]\(([^)]+)\)")
API_ROOT = "https://api.github.com"


class ReleaseApiError(Exception):
    """A release could not be fetched; never treat this as a parity result."""


def fetch_release(
    repo: str,
    tag: str,
    auth: str = "auto",
    *,
    environ: dict[str, str] | None = None,
    gh_path: str | None = None,
    runner=subprocess.run,
    opener=urllib.request.urlopen,
) -> tuple[dict, str]:
    """Fetch release JSON via GITHUB_TOKEN, authenticated gh, or public API."""
    env = os.environ if environ is None else environ
    token = env.get("GITHUB_TOKEN")
    cli = gh_path if gh_path is not None else shutil.which("gh")
    mode = auth
    if mode == "auto":
        mode = "token" if token else ("gh" if cli else "public")
    if mode == "token" and not token:
        raise ReleaseApiError("GITHUB_TOKEN auth requested but GITHUB_TOKEN is not set")
    if mode == "gh" and not cli:
        raise ReleaseApiError("gh auth requested but GitHub CLI is not available")

    endpoint = f"repos/{repo}/releases/tags/{tag}"
    if mode == "gh":
        try:
            result = runner(
                [cli, "api", endpoint], capture_output=True, text=True, encoding="utf-8", check=False
            )
        except OSError:
            raise ReleaseApiError("gh api could not be started; check GitHub CLI installation") from None
        if result.returncode != 0:
            raise ReleaseApiError(
                f"gh api failed (exit {result.returncode}); check GitHub CLI authentication and API access"
            )
        payload = result.stdout
        used = "gh api"
    else:
        headers = {"Accept": "application/vnd.github+json", "User-Agent": "nova-release-body-parity"}
        if mode == "token":
            headers["Authorization"] = f"Bearer {token}"
        request = urllib.request.Request(f"{API_ROOT}/{endpoint}", headers=headers)
        try:
            with opener(request, timeout=30) as response:
                payload = response.read().decode("utf-8")
        except urllib.error.HTTPError as exc:
            raise ReleaseApiError(f"GitHub API returned HTTP {exc.code} {exc.reason}") from None
        except urllib.error.URLError as exc:
            raise ReleaseApiError(f"GitHub API network error ({type(exc.reason).__name__})") from None
        used = "GITHUB_TOKEN" if mode == "token" else "unauthenticated GitHub API"

    try:
        release = json.loads(payload)
    except json.JSONDecodeError:
        raise ReleaseApiError(f"{used} returned invalid JSON") from None
    if not isinstance(release, dict) or not isinstance(release.get("body"), (str, type(None))):
        raise ReleaseApiError(f"{used} response is not a GitHub release object")
    return release, used


def normalize(text: str, repo: str, source_file: str, revision: str) -> str:
    """Pin relative Markdown destinations to blobs/tree at revision."""
    source_dir = posixpath.dirname(source_file.replace("\\", "/"))

    def replace(match: re.Match[str]) -> str:
        label, destination = match.groups()
        if destination.startswith(("https://", "http://", "mailto:", "#", "/")):
            return match.group(0)
        path, marker, fragment = destination.partition("#")
        path_part, query_marker, query = path.partition("?")
        resolved = posixpath.normpath(posixpath.join(source_dir, path_part))
        kind = "tree" if path_part.endswith("/") or not posixpath.splitext(path_part)[1] else "blob"
        if kind == "tree" and path_part.endswith("/"):
            resolved += "/"
        url = f"https://github.com/{repo}/{kind}/{revision}/{resolved}"
        suffix = (query_marker + query if query_marker else "") + (marker + fragment if marker else "")
        return f"[{label}]({url}{suffix})"

    return LINK.sub(replace, text)


def compare(body: str, source: str, repo: str, source_file: str, revision: str) -> bool:
    return normalize(body, repo, source_file, revision) == normalize(
        source, repo, source_file, revision
    )


def self_test() -> int:
    checked_in = "See [guide](../guide/tour.md#start).\n"
    release = "See [guide](https://github.com/acme/nova/blob/abc/docs/guide/tour.md#start).\n"
    assert compare(release, checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    checked_tree = "Browse [decisions](../../spec/decisions/).\n"
    release_tree = "Browse [decisions](https://github.com/acme/nova/tree/abc/spec/decisions/).\n"
    assert compare(release_tree, checked_tree, "acme/nova", "docs/dev/notes.md", "abc")
    print("SELFTEST GREEN: relative blob/tree links at the pinned revision compare equal")
    assert not compare(release + "changed\n", checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    print("SELFTEST RED (expected): arbitrary non-link text mutation is rejected")
    assert not compare(release.replace("\n", "\r\n"), checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    print("SELFTEST RED (expected): line-ending difference is rejected")
    assert not compare(release.replace("guide", "manual"), checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    print("SELFTEST RED (expected): altered link target is rejected")

    credential_sentinel = "opaque-sentinel-8721"

    def forbidden(_request, timeout):
        assert timeout == 30
        raise urllib.error.HTTPError("https://api.github.com/", 403, "rate limit", {}, None)

    try:
        fetch_release("acme/nova", "v1", "token", environ={"GITHUB_TOKEN": credential_sentinel}, opener=forbidden)
    except ReleaseApiError as exc:
        assert "403" in str(exc) and credential_sentinel not in str(exc)
        print("SELFTEST RED (expected): authenticated HTTP 403 is an API error; token is not logged")
    else:
        raise AssertionError("authenticated 403 was not raised as an API error")
    try:
        fetch_release("acme/nova", "v1", "token", environ={})
    except ReleaseApiError as exc:
        assert "GITHUB_TOKEN is not set" in str(exc)
        print("SELFTEST RED (expected): missing requested credentials fail explicitly")
    else:
        raise AssertionError("missing requested credentials were not reported")

    class FailedGh:
        returncode = 1
        stdout = ""
        stderr = credential_sentinel

    try:
        fetch_release("acme/nova", "v1", "gh", gh_path="gh", runner=lambda *_a, **_k: FailedGh())
    except ReleaseApiError as exc:
        assert "gh api failed" in str(exc) and credential_sentinel not in str(exc)
        print("SELFTEST RED (expected): gh auth/API failure is explicit and stderr/token are not logged")
    else:
        raise AssertionError("gh API failure was not raised")

    original_argv = sys.argv
    original_fetch = globals()["fetch_release"]
    captured = io.StringIO()
    try:
        sys.argv = [
            "release-body-parity.py", "--repo", "acme/nova", "--tag", "v1",
            "--file", str(Path(__file__)), "--source-revision", "abc",
        ]
        globals()["fetch_release"] = lambda *_a, **_k: (_ for _ in ()).throw(ReleaseApiError("GitHub API returned HTTP 403 Forbidden"))
        with contextlib.redirect_stderr(captured):
            status = main()
    finally:
        sys.argv = original_argv
        globals()["fetch_release"] = original_fetch
    assert status == 2 and "HTTP 403" in captured.getvalue() and "PARITY" not in captured.getvalue()
    print("SELFTEST RED (expected): API error exits 2, not parity mismatch/green")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="GitHub owner/repository")
    parser.add_argument("--tag", help="release tag")
    parser.add_argument("--file", type=Path, help="checked-in release-body Markdown file")
    parser.add_argument("--source-revision", help="revision used to pin relative links")
    parser.add_argument(
        "--api-auth", choices=("auto", "gh", "token", "public"), default="auto",
        help="GitHub API auth: auto prefers GITHUB_TOKEN, then gh, then public (default: auto)",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not all((args.repo, args.tag, args.file, args.source_revision)):
        parser.error("--repo, --tag, --file, and --source-revision are required")

    repo_root = Path(__file__).resolve().parents[2]
    try:
        source_file = args.file.resolve().relative_to(repo_root).as_posix()
        with args.file.open("r", encoding="utf-8", newline="") as source_stream:
            source = source_stream.read()
    except (OSError, ValueError) as exc:
        print(f"ERROR: source file must exist inside repository: {exc}", file=sys.stderr)
        return 2

    try:
        release, api_auth = fetch_release(args.repo, args.tag, args.api_auth)
    except ReleaseApiError as exc:
        print(f"ERROR: cannot fetch release {args.repo}@{args.tag}: {exc}", file=sys.stderr)
        return 2
    body = release.get("body") or ""
    normalized_body = normalize(body, args.repo, source_file, args.source_revision)
    normalized_source = normalize(source, args.repo, source_file, args.source_revision)
    matches = normalized_body == normalized_source
    print(f"URL: {release.get('html_url')}")
    print(f"SOURCE: {source_file} @ {args.source_revision}")
    print(f"API AUTH: {api_auth}")
    print(
        "LINE ENDINGS: release "
        f"CRLF={body.count(chr(13) + chr(10))}, LF={body.count(chr(10))}; "
        f"source CRLF={source.count(chr(13) + chr(10))}, LF={source.count(chr(10))}"
    )
    print("PARITY: PASS" if matches else "PARITY: FAIL (body differs beyond normalized relative Markdown links)")
    if not matches:
        print("DIFF (normalized release body vs checked-in source):")
        print("\n".join(difflib.unified_diff(
            normalized_body.splitlines(),
            normalized_source.splitlines(),
            fromfile="published-release-body", tofile=source_file,
            lineterm="",
        )))
    return 0 if matches else 1


if __name__ == "__main__":
    raise SystemExit(main())
