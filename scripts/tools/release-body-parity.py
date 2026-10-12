#!/usr/bin/env python3
"""Compare a GitHub release body with its checked-in Markdown source."""

from __future__ import annotations

import argparse
import difflib
import json
import posixpath
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path


LINK = re.compile(r"(?<!!)\[([^\]]*)\]\(([^)]+)\)")


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
    assert not compare(release + "changed\n", checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    assert not compare(release.replace("guide", "manual"), checked_in, "acme/nova", "docs/dev/notes.md", "abc")
    print("SELFTEST PASS: relative link normalization; arbitrary text/link mutations rejected")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="GitHub owner/repository")
    parser.add_argument("--tag", help="release tag")
    parser.add_argument("--file", type=Path, help="checked-in release-body Markdown file")
    parser.add_argument("--source-revision", help="revision used to pin relative links")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not all((args.repo, args.tag, args.file, args.source_revision)):
        parser.error("--repo, --tag, --file, and --source-revision are required")

    repo_root = Path(__file__).resolve().parents[2]
    try:
        source_file = args.file.resolve().relative_to(repo_root).as_posix()
        source = args.file.read_text(encoding="utf-8")
    except (OSError, ValueError) as exc:
        print(f"ERROR: source file must exist inside repository: {exc}", file=sys.stderr)
        return 2

    endpoint = f"https://api.github.com/repos/{args.repo}/releases/tags/{args.tag}"
    request = urllib.request.Request(endpoint, headers={"Accept": "application/vnd.github+json"})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            release = json.load(response)
    except (urllib.error.URLError, json.JSONDecodeError) as exc:
        print(f"ERROR: cannot fetch release {args.repo}@{args.tag}: {exc}", file=sys.stderr)
        return 2
    body = release.get("body") or ""
    normalized_body = normalize(body, args.repo, source_file, args.source_revision)
    normalized_source = normalize(source, args.repo, source_file, args.source_revision)
    matches = normalized_body == normalized_source
    print(f"URL: {release.get('html_url')}")
    print(f"SOURCE: {source_file} @ {args.source_revision}")
    print("PARITY: PASS" if matches else "PARITY: FAIL (body differs beyond normalized relative Markdown links)")
    if not matches:
        print("DIFF (normalized release body vs checked-in source):")
        print("".join(difflib.unified_diff(
            normalized_body.splitlines(keepends=True),
            normalized_source.splitlines(keepends=True),
            fromfile="published-release-body", tofile=source_file,
        )), end="")
    return 0 if matches else 1


if __name__ == "__main__":
    raise SystemExit(main())
