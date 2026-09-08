#!/usr/bin/env python3
"""Deep look at ONE neighbour session transcript: last K owner prompts (not tool results, not
cross-session), last assistant text, and whether the turn is closed.

Usage: python peers_deep.py <id-prefix> [K] [tail_mb]
"""
import glob
import io
import json
import os
import sys

CFG = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
prefix = sys.argv[1]
K = int(sys.argv[2]) if len(sys.argv) > 2 else 4
TAIL = int(float(sys.argv[3]) * 1e6) if len(sys.argv) > 3 else 6_000_000


def text_of(msg):
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


def trim(s, n):
    s = s.replace("\r", "").strip()
    return s if len(s) <= n else s[:n] + " ...[+%d]" % (len(s) - n)


files = [f for f in glob.glob(os.path.join(CFG, "projects", "*", "*.jsonl"))
         if os.path.basename(f).startswith(prefix)]
if len(files) != 1:
    print("matched %d files for prefix %s" % (len(files), prefix)); sys.exit(1)
path = files[0]
size = os.path.getsize(path)
with open(path, "rb") as f:
    if size > TAIL:
        f.seek(size - TAIL); f.readline()
    lines = f.read().decode("utf-8", "replace").splitlines()
entries = []
for ln in lines:
    try:
        entries.append(json.loads(ln))
    except Exception:
        pass

owner_prompts = []   # (ts, text, kind)
for e in entries:
    if e.get("type") != "user":
        continue
    msg = e.get("message") or {}
    txt = text_of(msg)
    if not txt.strip() or txt.strip().startswith("[tool_result]"):
        continue
    kind = "owner"
    if "<cross-session-message" in txt or "Another Claude session sent a message" in txt:
        kind = "peer-msg"
    elif txt.startswith("[Request interrupted"):
        kind = "interrupt"
    elif "<system-reminder>" in txt and len(txt.replace("<system-reminder>", "").strip()) < 20:
        kind = "system"
    elif "Cross-session idle notice" in txt:
        kind = "idle-notice"
    elif txt.lstrip().startswith("<task-notification") or "task-notification" in txt[:200]:
        kind = "task-notif"
    owner_prompts.append((e.get("timestamp"), txt, kind))

last_asst = None
for e in reversed(entries):
    if e.get("type") == "assistant":
        txt = text_of(e.get("message") or {})
        if txt.strip():
            last_asst = (e.get("timestamp"), txt); break

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
print("file=%s size=%.1fMB entries_in_tail=%d" % (os.path.basename(path), size / 1e6, len(entries)))
print("last entry type=%s ts=%s" % (entries[-1].get("type") if entries else None, entries[-1].get("timestamp") if entries else None))
print()
print("### last %d non-tool USER inputs (oldest first):" % K)
for ts, txt, kind in owner_prompts[-K:]:
    print("--- [%s] %s" % (kind, ts))
    print(trim(txt, 1500))
print()
print("### last ASSISTANT text @ %s:" % (last_asst[0] if last_asst else None))
print(trim(last_asst[1] if last_asst else "", 3000))
