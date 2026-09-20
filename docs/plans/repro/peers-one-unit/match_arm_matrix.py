# -*- coding: utf-8 -*-
"""Which match-arm patterns does the parser read? A matrix, one file each."""
import io
import json
import os
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

B = os.path.dirname(os.path.abspath(__file__)).replace(os.sep, "/") + "/arms"
env = dict(os.environ)
env["NOVAC_SELF_PATH"] = "novac/src"

HEAD = "module probe\n\ntype K enum A | B\n\n"

CASES = {
    # the shapes the first-fallback list showed, each on its own
    "bare_variant": "fn f(k K) -> int => match k {\n    A => 1\n    B => 2\n}\n",
    "qualified_variant": "fn f(k K) -> int => match k {\n    K.A => 1\n    K.B => 2\n}\n",
    "wildcard": "fn f(k K) -> int => match k {\n    _ => 1\n}\n",
    "payload_bind": "fn f(o Option[int]) -> int => match o {\n    Some(v) => v\n    None => 0\n}\n",
    "payload_wild": "fn f(o Option[int]) -> int => match o {\n    Some(_) => 1\n    None => 0\n}\n",
    "arm_returns": "fn f(o Option[int]) -> int {\n    match o {\n        Some(v) => { return v }\n        None => { return 0 }\n    }\n}\n",
    "arm_bare_return": "fn f(o Option[int]) -> int {\n    match o {\n        Some(v) => return v\n        None => return 0\n    }\n}\n",
    "str_literal_arm": "fn f(s str) -> int => match s {\n    \"a\" => 1\n    _ => 0\n}\n",
    "char_literal_arm": "fn f(c char) -> int => match c {\n    'a' => 1\n    _ => 0\n}\n",
    "int_literal_arm": "fn f(n int) -> int => match n {\n    1 => 1\n    _ => 0\n}\n",
    "tuple_payload": "fn f(o Option[int]) -> int => match o {\n    Some(v) => v\n    None => 0\n}\n",
}

for name, body in CASES.items():
    d = "%s/%s" % (B, name)
    if not os.path.isdir(d):
        os.makedirs(d)
    p = d + "/probe.nv"
    io.open(p, "w", encoding="utf-8", newline="\n").write(HEAD + body)
    out = subprocess.run(["./novac/target/novac.exe", "check", p], env=env,
                         capture_output=True, text=True,
                         encoding="utf-8", errors="replace").stdout
    msgs = []
    for line in out.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            msgs.append(json.loads(line).get("message", ""))
        except ValueError:
            msgs.append("RAW " + line[:80])
    tag = "ACCEPTED" if not msgs else ("PARSE-STOP" if any(
        "did not parse" in m for m in msgs) else "refused")
    print("%-20s %-11s %s" % (name, tag, (msgs[0][:86] if msgs else "")))
