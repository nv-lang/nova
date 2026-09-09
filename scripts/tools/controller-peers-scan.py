#!/usr/bin/env python3
"""Scan neighbouring Claude Code session transcripts: who works where, and did the last turn end on a report.

Usage: python peers_scan.py [N] [--full ID_PREFIX]
Reads $CLAUDE_CONFIG_DIR/projects/*/*.jsonl, newest N by mtime (default 8), skipping this session.
Prints per session: id, project dir, age, size, cwd, last user text, last assistant text, last entry kind.
"""
import glob
import io
import json
import os
import sys
import time

CFG = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
ME = os.environ.get("CLAUDE_CODE_SESSION_ID", "")
N = 8
FULL = None
args = sys.argv[1:]
if args and args[0].isdigit():
    N = int(args[0]); args = args[1:]
if len(args) >= 2 and args[0] == "--full":
    FULL = args[1]

TAIL_BYTES = 400_000
HEAD_BYTES = 60_000


def read_tail_lines(path, nbytes=TAIL_BYTES):
    size = os.path.getsize(path)
    with open(path, "rb") as f:
        if size > nbytes:
            f.seek(size - nbytes)
            f.readline()  # drop partial
        data = f.read()
    return data.decode("utf-8", "replace").splitlines()


def read_head_lines(path, nbytes=HEAD_BYTES):
    with open(path, "rb") as f:
        data = f.read(nbytes)
    lines = data.decode("utf-8", "replace").splitlines()
    return lines[:-1] if len(data) == nbytes else lines


def parse(lines):
    out = []
    for ln in lines:
        ln = ln.strip()
        if not ln:
            continue
        try:
            out.append(json.loads(ln))
        except Exception:
            pass
    return out


def text_of(msg):
    """Extract plain text from a message content (string or blocks)."""
    c = msg.get("content")
    if isinstance(c, str):
        return c
    parts = []
    if isinstance(c, list):
        for b in c:
            if not isinstance(b, dict):
                continue
            t = b.get("type")
            if t == "text":
                parts.append(b.get("text", ""))
            elif t == "tool_use":
                parts.append("[tool_use %s]" % b.get("name"))
            elif t == "tool_result":
                parts.append("[tool_result]")
    return "\n".join(parts)


def kind_of(entry):
    msg = entry.get("message") or {}
    c = msg.get("content")
    kinds = []
    if isinstance(c, list):
        for b in c:
            if isinstance(b, dict):
                kinds.append(b.get("type", "?"))
    elif isinstance(c, str):
        kinds.append("str")
    return entry.get("type", "?") + ":" + ",".join(kinds)


def trim(s, n=600):
    s = s.replace("\r", "").strip()
    if len(s) > n:
        return s[:n] + " ...[+%d]" % (len(s) - n)
    return s


def scan_one(path):
    tail = parse(read_tail_lines(path))
    head = parse(read_head_lines(path))
    cwd = None
    sid = None
    for e in head + tail:
        if not cwd and e.get("cwd"):
            cwd = e["cwd"]
        if not sid and e.get("sessionId"):
            sid = e["sessionId"]
        if cwd and sid:
            break
    last_user = None
    last_asst = None
    last_asst_ts = None
    last_user_ts = None
    for e in reversed(tail):
        t = e.get("type")
        msg = e.get("message") or {}
        if t == "assistant" and last_asst is None:
            txt = text_of(msg)
            if txt.strip():
                last_asst = txt
                last_asst_ts = e.get("timestamp")
        if t == "user" and last_user is None:
            txt = text_of(msg)
            # skip pure tool_result entries
            if txt.strip() and txt.strip() != "[tool_result]" and not txt.startswith("[tool_result]"):
                last_user = txt
                last_user_ts = e.get("timestamp")
        if last_user and last_asst:
            break
    last_entry = tail[-1] if tail else {}
    return dict(cwd=cwd, sid=sid, last_user=last_user, last_asst=last_asst,
                last_asst_ts=last_asst_ts, last_user_ts=last_user_ts,
                last_kind=kind_of(last_entry), last_ts=last_entry.get("timestamp"),
                n_tail=len(tail))


def main():
    files = glob.glob(os.path.join(CFG, "projects", "*", "*.jsonl"))
    files = [f for f in files if os.path.basename(f) != ME + ".jsonl"]
    files.sort(key=os.path.getmtime, reverse=True)
    now = time.time()
    if FULL:
        files = [f for f in files if os.path.basename(f).startswith(FULL)]
    for f in files[:N]:
        st = os.stat(f)
        age_min = (now - st.st_mtime) / 60
        info = scan_one(f)
        print("=" * 100)
        print("id=%s  proj=%s" % (os.path.basename(f)[:-6], os.path.basename(os.path.dirname(f))))
        print("mtime_age=%.1f min  size=%.1f MB  cwd=%s" % (age_min, st.st_size / 1e6, info["cwd"]))
        print("last_entry=%s @ %s" % (info["last_kind"], info["last_ts"]))
        print("-- last USER (%s):" % info["last_user_ts"])
        print(trim(info["last_user"] or "", 500 if not FULL else 4000))
        print("-- last ASSISTANT text (%s):" % info["last_asst_ts"])
        print(trim(info["last_asst"] or "", 900 if not FULL else 6000))
    print("=" * 100)
    print("scanned=%d of %d files; me=%s" % (min(N, len(files)), len(files), ME))


if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    main()
