#!/usr/bin/env python3
"""Самотест guard-blocking-wait.py — обе стороны и клапан.

Перехватчик, который не пропускает обычные команды, остановит работу; который не
ловит цикл ожидания — бесполезен. Носитель — дословный цикл 2026-10-02.
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
HOOK = os.path.join(os.path.dirname(HERE), "guard-blocking-wait.py")
PASS = 0
FAIL = 0


def run(cmd, background=False):
    ti = {"command": cmd}
    if background:
        ti["run_in_background"] = True
    p = subprocess.run([sys.executable, HOOK], input=json.dumps({"tool_input": ti}).encode("utf-8"),
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return p.returncode


def check(label, got, want):
    global PASS, FAIL
    if got == want:
        PASS += 1
        print("  ok   %s" % label)
    else:
        FAIL += 1
        print("  FAIL %s (want %s, got %s)" % (label, want, got), file=sys.stderr)


CARRIER = ('F="/d/x.output"; for i in $(seq 1 27); do grep -qE \'committed|RED\' "$F" && break; '
           'sleep 20; done; date +%H:%M; cat "$F"')

print("== blokiruet ==")
check("nositel 2026-10-02: for + sleep 20", run(CARRIER), 2)
check("while + sleep 5", run("while ! test -f /d/x; do sleep 5; done"), 2)
check("until + sleep 30", run("until grep -q done log; do sleep 30; done"), 2)
check("odinochnyi sleep 120", run("sleep 120; echo hi"), 2)
check("PowerShell while + Start-Sleep", run("while (-not (Test-Path x)) { Start-Sleep -Seconds 10 }"), 2)
check("PowerShell Start-Sleep 90", run("Start-Sleep 90"), 2)

print("== propuskaet ==")
check("obychnaya komanda", run("git status --porcelain"), 0)
check("for bez sleep", run("for f in a b c; do echo $f; done"), 0)
check("korotkii sleep 2 bez cikla", run("kill 123; sleep 2; ps -ef | wc -l"), 0)
check("fon: tot zhe nositel s run_in_background", run(CARRIER, background=True), 0)
check("slovo sleep v stroke grep", run("grep -n 'sleepy' file.txt"), 0)
check("pustaya komanda", run(""), 0)

print("== klapan ==")
check("blocking-wait-ok s prichinoi", run("for i in 1 2; do sleep 3; done # blocking-wait-ok: probe of a race, 6 s"), 0)
check("blocking-wait-ok bez prichiny ne klapan", run("for i in 1 2; do sleep 3; done # blocking-wait-ok:"), 2)

print("test-guard-blocking-wait: %s (PASS %d FAIL %d)" % ("ok" if FAIL == 0 else "FAIL", PASS, FAIL))
sys.exit(0 if FAIL == 0 else 1)
