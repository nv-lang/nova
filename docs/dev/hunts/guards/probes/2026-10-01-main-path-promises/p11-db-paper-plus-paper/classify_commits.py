# -*- coding: utf-8 -*-
# Read-only measure over git history: of the non-merge commits since a date,
# which ones the docs-batched set would judge (all paths in the paper set), and
# which are docs-only (every path under docs/ or the registry baseline) yet
# carry a path OUTSIDE the set, so the guard says "not ours".
# Usage: python classify_commits.py <repo> <since>
import fnmatch, subprocess, sys
from collections import Counter

repo, since = sys.argv[1], sys.argv[2]

def in_set(p):
    if p in ("docs/plans/221.1-bug-sweep.md", "scripts/guards/registry-rows.baseline"):
        return True
    if p.startswith("docs/dev/prompts/"):
        rest = p[len("docs/dev/prompts/"):]
        if "/" in rest:
            return False
        return fnmatch.fnmatchcase(rest, "*-handoff.md") or fnmatch.fnmatchcase(rest, "controller-*.md")
    return False

def docs_only(p):
    return p.startswith("docs/") or p == "scripts/guards/registry-rows.baseline"

log = subprocess.run(["git", "-C", repo, "log", "--since=" + since, "--no-merges",
                      "--first-parent", "--format=@@%h", "--name-only", "--no-renames"],
                     capture_output=True, text=True, encoding="utf-8").stdout
commits, cur = [], None
for line in log.splitlines():
    if line.startswith("@@"):
        cur = [line[2:], []]
        commits.append(cur)
    elif line.strip() and cur is not None:
        cur[1].append(line.strip())
judged = escaped = mixed = 0
outside = Counter()
for h, paths in commits:
    if not paths:
        continue
    if all(in_set(p) for p in paths):
        judged += 1
    elif all(docs_only(p) for p in paths):
        escaped += 1
        if any(in_set(q) for q in paths):
            mixed += 1
        for p in paths:
            if not in_set(p):
                outside[p] += 1
print("first-parent non-merge commits since %s: %d" % (since, len(commits)))
print("all paths in the paper set (guard would judge): %d" % judged)
print("docs-only but with a path outside the set (guard: not ours): %d" % escaped)
print("  of them carrying an in-set path too (registry/handoff + other paper): %d" % mixed)
print("most frequent outside paths:")
for p, c in outside.most_common(12):
    print("  %3d %s" % (c, p))
