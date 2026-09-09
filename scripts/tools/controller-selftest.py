#!/usr/bin/env python3
"""Self-test of the window-controller role: checks the command file against its OWN requirements.

Why it exists: every rule in `.claude/commands/controller.md` was bought with a mistake, and a
rule nobody checks decays into advice. This runs the checks the command demands of itself, and
prints a TABLE (owner's standing requirement: reports are tables).

Two properties it must have, or it is theatre:
  * it must go RED when a requirement is violated -- each check is proven by a negative probe
    against a synthetic sample, not only by passing on the live file (`--prove`);
  * it must never judge by an exit code alone (registry row #1040: `nova test --filter` returns
    rc=0 having run nothing). So the verdict is the table plus a final line, and rc mirrors it.

Usage:
  python controller-selftest.py            # check the live command file, print the table
  python controller-selftest.py --prove    # additionally prove every check reddens
  python controller-selftest.py --cmd PATH # check a different file (used by --prove)

Exit: 0 = all required checks pass, 1 = at least one fails.
"""
import io
import os
import re
import subprocess
import sys
import time

# The repository root is DERIVED from this file's own location, not written down.
# A hardcoded absolute path is a measurement taken on one machine and recorded as a
# rule -- the same class that made the machine watch print "FREE" on Linux an hour
# before this landed (it knew only `nova.exe`). Here it would be quieter still: in a
# worktree or on anyone else's checkout the tool would read a DIFFERENT tree's command
# file and report on it as if it were this one. This script lives in scripts/tools/,
# so the root is two levels up; NOVA_REPO_ROOT overrides for a deliberate cross-check.
REPO = os.environ.get(
    "NOVA_REPO_ROOT",
    os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
)
CMD_DEFAULT = os.path.join(REPO, ".claude", "commands", "controller.md")
TOOLS = os.path.join(REPO, "scripts", "tools")
TOOL_NAMES = ["controller-dock.py", "controller-peers-scan.py",
              "controller-peers-deep.py", "controller-machine-watch.py"]


def read(path):
    try:
        return io.open(path, encoding="utf-8").read()
    except Exception as e:  # noqa: BLE001
        return "<<UNREADABLE: %s>>" % e


# --- the checks: (id, what the command requires of itself, predicate over the text) -----------

def c_frontmatter(t, _):
    """Header with a description: the project's guard demands it of every command."""
    return t.startswith("---") and "description:" in t.split("---")[1]


def c_role_first(t, _):
    """Role and its limits stated before any procedure -- a cycle reads top-down."""
    head = t[:1500]
    return ("контрол" in head) and ("не коммич" in head or "только чтение" in head)


def c_docking(t, _):
    """STEP 0 docking, because peer names and ids change on restart."""
    return "СТЫКОВКА" in t and "controller-dock.py" in t


def c_time_both_scales(t, _):
    """Time in every report, both scales -- artifacts are local time, histories are UTC."""
    return ("date -u" in t and "date +" in t) and ("UTC" in t and ("мест" in t or "+0300" in t))


def c_table_form(t, _):
    """Report form: a table with the four columns, status in two-three words."""
    return ("окно | что делает | стоит | статус" in t) and "ТАБЛИЦ" in t


def c_push_form(t, _):
    """Push form: self-contained first line, ПЕРЕДАЮ with the third part, ПРОШУ."""
    return all(k in t for k in ("ПЕРЕДАЮ", "ПРОШУ", "что это меняет"))


def c_legit_stall(t, _):
    """Three signs of a legitimate stall, all required at once."""
    return "ТРИ признака" in t or "три признака" in t


def c_promise_not_work(t, _):
    """A promise is not work: status `работает` needs an action after the last text."""
    return "ОБЕЩАНИЕ" in t and ("mtime" in t or "ls -l" in t or "время изменения" in t)


def c_answer_not_turn(t, _):
    """An answer to the owner is not a turn by the plan."""
    return "ОТВЕТ ВЛАДЕЛЬЦУ" in t


def c_frozen_tree(t, _):
    """While a tier runs, the main tree changes for nobody."""
    return "НЕ МЕНЯЕТСЯ НИКЕМ" in t or "не меняется никем" in t


def c_origin_not_local(t, _):
    """Measure `behind` against origin/main, the gated state, not a local main."""
    return "origin/main" in t and "HEAD..origin/main" in t


def c_no_second_copy(t, _):
    """The command declares it is the single home of the rules -- and says how to verify."""
    return "ВТОРАЯ КОПИЯ" in t and "after-compact.list" in t


def c_no_cyrillic_in_shell(t, _):
    """No Russian words in a bash command line: the project hook cuts such a command."""
    return "ASCII" in t or "русского слова" in t


def c_cronlist_each_cycle(t, _):
    """CronList is part of the cycle: a stale job carrying the rules ran for a whole shift.

    Measured 01:04Z 2026-09-09: two jobs on the same schedule, the older one holding the FULL
    rule text inside itself -- the very second copy this command calls the role's costliest
    mistake. It was found by accident (a cycle arrived one minute after the previous one), not
    by any check. So the command must demand the check every cycle, not once at setup.
    """
    return "CronList" in t and ("CronDelete" in t or "РОВНО ОДНУ" in t)


def c_tools_in_repo(t, _):
    """Tools live in the repository, each named in the command."""
    return all(("scripts/tools/" + n) in t.replace("\\", "/") for n in TOOL_NAMES)


def c_agent_models_line(t, _):
    """Reports name the agent models -- otherwise the saving cannot be verified."""
    return "Модели агентов" in t


CHECKS = [
    ("frontmatter", "shapka s description (guard proekta)", c_frontmatter, True),
    ("role-first", "rol i granicy do procedury", c_role_first, True),
    ("docking", "SHAG 0 stykovka posle restarta", c_docking, True),
    ("time-2-scales", "vremya v otchete, obe shkaly", c_time_both_scales, True),
    ("table-form", "forma doklada: tablica, 4 kolonki", c_table_form, True),
    ("push-form", "forma tolchka: PEREDAYU/PROSHU + tretya chast", c_push_form, True),
    ("legit-stall", "tri priznaka zakonnoy stoyanki", c_legit_stall, True),
    ("promise-not-work", "obeshchanie ne rabota: status po deystviyu", c_promise_not_work, True),
    ("answer-not-turn", "otvet vladelcu ne hod po planu", c_answer_not_turn, True),
    ("frozen-tree", "pod yarusom derevo ne menyaetsya", c_frozen_tree, True),
    ("origin-not-local", "otstavanie merit ot origin/main", c_origin_not_local, True),
    ("no-second-copy", "odin dom pravil + kak proverit", c_no_second_copy, True),
    ("no-cyrillic-shell", "zapret russkih slov v komandnoy stroke", c_no_cyrillic_in_shell, True),
    ("cronlist-cycle", "CronList kazhdyy cikl, rovno odna stroka", c_cronlist_each_cycle, True),
    ("tools-in-repo", "instrumenty v scripts/tools, vse nazvany", c_tools_in_repo, True),
    ("agent-models", "stroka Modeli agentov v doklade", c_agent_models_line, True),
]


def subject_of(path):
    """Does any guard actually JUDGE this file? Printed next to every late edit.

    Ordered by the integrator (04:39 local 2026-09-09) after both of us spent two cycles on the
    wrong question. We were comparing "did the guard read the file before or after the edit",
    when the first question is "is this guard about this file at all". His third fact settled it:
    `controller.md` is NOT in `.claude/after-compact.list`, the context-layer guard counts three
    files after compaction and five at start, and my command is in neither list -- so the budget
    guard never judged it, whichever side of the clock the edit fell on. His own verdict of the
    previous cycle, he says, was right by numbers and by accident.

    Cheap and honest: name the lists this file provably belongs to, and say "no known subject"
    rather than guessing. An unknown answer is reported as unknown, never as "safe".
    """
    rel = os.path.relpath(path, REPO).replace("\\", "/")
    subjects = []
    lst = os.path.join(REPO, ".claude", "after-compact.list")
    try:
        entries = [l.strip() for l in io.open(lst, encoding="utf-8").read().splitlines()]
        if any(e and not e.startswith("#") and e.strip("/") in rel for e in entries):
            subjects.append("context-layer")
    except Exception:  # noqa: BLE001
        subjects.append("after-compact.list UNREADABLE")
    # scripts/ is the subject of the EOL and shebang guards -- they walk the directory, so
    # membership is by location, not by a list.
    if rel.startswith("scripts/"):
        subjects.append("script-eol/mixed-eol")
    return ",".join(subjects) if subjects else "no known subject"


def freeze_state():
    """LIVE check, not a text check: is a tier running, and did my files change after it started?

    Ordered by the integrator (04:09 local 2026-09-09) after I broke prohibition #15 twice in one
    shift: "a rule broken twice by the one who wrote it does not need a stricter wording, it needs
    a mechanism." His measurement is also the reason this reports times rather than a verdict --
    at 01:02Z my edit landed on the 60th second of his tier, but the two guards that judge that
    file ran at 255s and 269s, so they read the NEW file and a rollback would have restored a
    state the gate never saw. Whether a late edit spoils the verdict is decided by two numbers --
    when the guard read the file and when the file changed -- and the second belongs to him.

    Returns (note, ok): ok=False only when a tier is running AND a controller file is newer than
    its start, which is the case the prohibition forbids.
    """
    try:
        out = subprocess.run(["ps", "-ef"], capture_output=True, timeout=30)
        rows = out.stdout.decode("utf-8", errors="replace").splitlines()
    except Exception:  # noqa: BLE001
        return "ps unavailable -- freeze state UNKNOWN, do not treat as free", True

    # ONE pattern with the machine-watch: a run that starts with a build holds the slot before
    # gate.sh comes up. Measured 01:57Z 2026-09-09 by comparing my reading with window 274's --
    # they saw `nova` at 04:49:48 while I called the tier's start 04:51:37, a 109-second window
    # in which an edit would be reported as "before the tier" though the run was already reading
    # the tree. It then showed itself LIVE at 02:23Z: freeze-now said "tree may be edited" while
    # the integrator's `cargo test --release` was running, and what stopped me was his freeze,
    # not this check. A check whose job is done in the crucial moment by someone else's word is
    # not working, however few errors stand against it.
    SLOT = re.compile(r"(gate\.sh|gate-novac|nova test|(?:release|target)[/\\]novac?(?:\.exe)?(?![\w.])|cargo|clang|cl\.exe|link\.exe)")
    tiers = [r for r in rows if SLOT.search(r) and "grep" not in r and "controller-" not in r]
    if not tiers:
        return "no tier running -- tree may be edited", True

    # `ps -ef` STIME is a clock (HH:MM:SS) for a process started today. Take the earliest.
    starts = []
    for r in tiers:
        m = re.search(r"\s(\d{2}:\d{2}:\d{2})\s", r)
        if m:
            starts.append(m.group(1))
    if not starts:
        return "tier running, start time unreadable -- ask the integrator", True
    start = min(starts)

    # Compare MOMENTS, not the text of clocks. Caught by the both-ways probe at 01:13Z 2026-09-09,
    # the first time it ran: a file last touched YESTERDAY at 20:xx compared as ">" against a tier
    # started TODAY at 04:xx, because "20" sorts after "04" -- so an untouched file was reported as
    # edited under the tier. The very class this role watches for: the check answered a question
    # about strings while claiming to answer one about time.
    now = time.localtime()
    h, mi, s = (int(x) for x in start.split(":"))
    start_ts = time.mktime((now.tm_year, now.tm_mon, now.tm_mday, h, mi, s, 0, 0, -1))

    watched = [CMD_DEFAULT] + [os.path.join(TOOLS, n) for n in TOOL_NAMES + ["controller-selftest.py"]]
    newer = []
    for p in watched:
        if not os.path.isfile(p):
            continue
        mts = os.path.getmtime(p)
        if mts > start_ts:
            newer.append("%s@%s[%s]" % (os.path.basename(p),
                                        time.strftime("%H:%M:%S", time.localtime(mts)),
                                        subject_of(p)))
    if newer:
        return "TIER since %s, CHANGED AFTER IT: %s -- tell the integrator" % (start, ",".join(newer)), False
    return "tier since %s, no controller file newer" % start, True


def tools_present():
    """Tools named by the command must exist and parse. Reported as a separate row."""
    missing, broken = [], []
    for n in TOOL_NAMES:
        p = os.path.join(TOOLS, n)
        if not os.path.isfile(p):
            missing.append(n)
            continue
        r = subprocess.run([sys.executable, "-c",
                            "import ast,sys;ast.parse(open(sys.argv[1],encoding='utf-8').read())", p],
                           capture_output=True, text=True)
        if r.returncode != 0:
            broken.append(n)
    return missing, broken


def run(cmd_path, show_table=True):
    t = read(cmd_path)
    rows = []
    for cid, what, fn, required in CHECKS:
        try:
            ok = bool(fn(t, cmd_path))
        except Exception:  # noqa: BLE001
            ok = False
        rows.append((cid, what, ok, required))

    missing, broken = tools_present()
    tools_ok = not missing and not broken
    tools_note = "vse 4 na meste i parsyatsya"
    if missing:
        tools_note = "NET: " + ",".join(missing)
    elif broken:
        tools_note = "SYNTAX: " + ",".join(broken)
    rows.append(("tools-exist", tools_note, tools_ok, False))

    # Live state, not text: the mechanism the integrator asked for in place of a stricter wording.
    freeze_note, freeze_ok = freeze_state()
    rows.append(("freeze-now", freeze_note, freeze_ok, False))

    if show_table:
        print("| proverka | trebovanie komandy | itog |")
        print("|---|---|---|")
        for cid, what, ok, required in rows:
            mark = "OK" if ok else ("FAIL" if required else "WARN")
            print("| %s | %s | %s |" % (cid, what, mark))

    failed = [r[0] for r in rows if r[3] and not r[2]]
    return rows, failed


def prove():
    """Every check must REDDEN on a sample where its requirement is violated."""
    t = read(CMD_DEFAULT)
    print()
    print("| proverka | probe (chto ubrano iz obrazca) | krasneet? |")
    print("|---|---|---|")
    bad = 0
    # For each check, build a sample with its key evidence removed, and require the check to fail.
    kills = {
        "frontmatter": lambda s: s.split("---", 2)[-1],
        "role-first": lambda s: s.replace("не коммич", "XX").replace("только чтение", "XX")[:1500] + s[1500:],
        "docking": lambda s: s.replace("controller-dock.py", "XX").replace("СТЫКОВКА", "XX"),
        "time-2-scales": lambda s: s.replace("date -u", "XX"),
        "table-form": lambda s: s.replace("окно | что делает | стоит | статус", "XX"),
        "push-form": lambda s: s.replace("что это меняет", "XX"),
        "legit-stall": lambda s: s.replace("ТРИ признака", "XX").replace("три признака", "XX"),
        "promise-not-work": lambda s: s.replace("ОБЕЩАНИЕ", "XX"),
        "answer-not-turn": lambda s: s.replace("ОТВЕТ ВЛАДЕЛЬЦУ", "XX"),
        "frozen-tree": lambda s: s.replace("НЕ МЕНЯЕТСЯ НИКЕМ", "XX").replace("не меняется никем", "XX"),
        "origin-not-local": lambda s: s.replace("HEAD..origin/main", "XX"),
        "no-second-copy": lambda s: s.replace("after-compact.list", "XX"),
        "no-cyrillic-shell": lambda s: s.replace("ASCII", "XX").replace("русского слова", "XX"),
        "cronlist-cycle": lambda s: s.replace("CronList", "XX"),
        "tools-in-repo": lambda s: s.replace("scripts/tools/controller-dock.py", "XX"),
        "agent-models": lambda s: s.replace("Модели агентов", "XX"),
    }
    by_id = {cid: fn for cid, _, fn, _ in CHECKS}
    for cid, kill in kills.items():
        sample = kill(t)
        still_ok = False
        try:
            still_ok = bool(by_id[cid](sample, CMD_DEFAULT))
        except Exception:  # noqa: BLE001
            still_ok = False
        reddens = not still_ok
        if not reddens:
            bad += 1
        print("| %s | %s | %s |" % (cid, "key evidence removed", "YES" if reddens else "NO -- USELESS CHECK"))
    print()
    print("probe summary: checks that fail to redden: %d" % bad)
    return bad


def main():
    args = sys.argv[1:]
    cmd = CMD_DEFAULT
    if "--cmd" in args:
        cmd = args[args.index("--cmd") + 1]
    print("controller-selftest: %s" % cmd)
    print()
    rows, failed = run(cmd)
    bad_probes = prove() if "--prove" in args else 0

    print()
    if failed:
        print("SELFTEST FAIL: trebovaniya ne vypolneny: %s" % ",".join(failed))
    elif bad_probes:
        print("SELFTEST FAIL: %d proverok ne krasneyut -- oni bespolezny" % bad_probes)
    else:
        n = sum(1 for r in rows if r[2])
        print("SELFTEST OK: %d/%d proverok proshli%s" % (
            n, len(rows), ", vse krasneyut na probe" if "--prove" in args else ""))
    return 1 if (failed or bad_probes) else 0


if __name__ == "__main__":
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")
    sys.exit(main())
