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
    ("tools-in-repo", "instrumenty v scripts/tools, vse nazvany", c_tools_in_repo, True),
    ("agent-models", "stroka Modeli agentov v doklade", c_agent_models_line, True),
]


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
