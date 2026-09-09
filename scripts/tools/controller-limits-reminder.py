#!/usr/bin/env python3
"""Is it time to remind the peer windows about limits and /delegate -- and what exactly to send.

Owner's order, 13:04Z 2026-09-09: "periodically (not less than once an hour) remind the windows to
save limits and work within /delegate". Second order minutes later: do it THROUGH A SCRIPT. Same
reason the status verdict became a program -- a cadence kept by the window's attention is not a
cadence. The cycle runs every seven minutes and the window does not remember its own past turns,
so by hand this degenerates into either a reminder every cycle (itself the waste being warned
about) or none at all.

What this tool decides, and what it deliberately does NOT do:
  DECIDES  -- whether the hour has elapsed (stamp file vs now), WHO is alive to receive it, and
              the exact text per recipient: a common part plus a part specific to that window.
  DOES NOT -- send. Cross-session delivery is a tool call belonging to the window; a script cannot
              make it. So this prints ready-to-send blocks and the window sends them verbatim.
              Claiming otherwise would be a tool answering a question it was not asked, which is
              the class this whole role exists to catch.

Usage (from the main tree root):
    python scripts/tools/controller-limits-reminder.py           # decide, and print what to send
    python scripts/tools/controller-limits-reminder.py --force   # print regardless of the hour
    python scripts/tools/controller-limits-reminder.py --sent     # record that it went out
"""
import datetime as _dt
import glob
import importlib.util
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
# The root is DERIVED, never written down: a hardcoded path makes the tool read somebody else's
# tree from a worktree and report on it as its own (integrator's lesson, 21:41 local 2026-09-08).
ROOT = os.environ.get("NOVA_REPO_ROOT") or os.path.abspath(os.path.join(HERE, "..", ".."))

STAMP_ENV = "CONTROLLER_REMINDER_STAMP"
# State of the ROLE, not an artefact of the repository -- and this line is a fix, made 15:06Z
# 2026-09-09, the minute the tool moved into `scripts/tools/`. Before the move both the stamp and
# the letters lived beside the script in the scratchpad; after it they resolved into the MAIN TREE,
# so (a) the previous stamp became invisible and the first run announced "no stamp -- send now",
# resetting the owner's hourly clock to zero without saying so, and (b) russian-text letters would
# have been written into a tree that a forty-minute tier judges. Both are the class this role
# hunts: the measurement kept its shape and changed what it looked at. The scratchpad is derived
# from the environment, never from the script's own location.
def _state_dir():
    for var in ("CLAUDE_SCRATCHPAD", "CLAUDE_SCRATCHPAD_DIR", "TMPDIR", "TEMP", "TMP"):
        v = os.environ.get(var)
        if v and os.path.isdir(v):
            d = os.path.join(v, "controller-reminder") if "SCRATCHPAD" not in var else v
            try:
                os.makedirs(d, exist_ok=True)
                return d
            except OSError:
                continue
    return HERE  # last resort, and the caller is told which directory it got


STATE_DIR = _state_dir()
DEFAULT_STAMP = os.path.join(STATE_DIR, "last-limits-reminder.txt")
INTERVAL_SEC = 3600  # owner's ceiling: "not less than once an hour"

COMMON = """ЛИМИТЫ - РЕСУРС, И ОН КОНЧАЕТСЯ; работа с агентами - строго в рамках `/delegate`.

Дом правила - `.claude/commands/delegate.md`, и перечитывать его надо перед КАЖДЫМ заданием, а не
по памяти: там измеренные ограничения среды, а не пожелания. Кратко: модель - самая дешёвая из
тех, что справится (haiku - механика по образцу: перечисления, сверки, замены по точному списку,
сбор таблицы фактов; sonnet - карты и инвентари; opus - только разведка и решения). В задании
дословно: запрет спавнить суб-агентов, запрет заходить в главное дерево иначе как `git -C`,
запрет на любые сборки и полный `nova test`, ОБРАЗЕЦ строки результата. В отчёте владельцу -
строка «Модели агентов: ...», иначе экономию нельзя ни проверить, ни повторить."""

# The third part is per-window and MUST be specific: a form letter teaches the neighbour not to
# read letters, which is the same waste the letter warns about (the rule "an empty cycle means
# ZERO messages" says it from the other side). Keyed by TREE, not by name -- names change without
# the sessions restarting at all (measured 00:45Z 2026-09-09).
PER_TREE = {
    "nova-p274": (
        "Ваша работа последних ходов - вычитка плана и чтение эмиттера. Чтение по ОБРАЗЦУ "
        "(перечислить методы, свести таблицу «метод -> что печатает») - первая строка списка "
        "haiku; суждение «уезжает или нет» остаётся вашим и не делегируется."),
    "claude-limits": (
        "Ваш проход по фазе - сверка карточек с точным списком условий, то есть буквально "
        "механика по образцу, и образец вы задали сами (машинная и глазная половины, «ЧАСТИЧНО "
        "n из m»). Себе оставить суждение «дефект или норма»: приёмка отчёта агента не "
        "делегируется, отчёт-перечисление - сырьё, а не вердикт."),
    "nova": (
        "Разбор отказов яруса - самая дорогая читательская работа смены. Сбор таблицы «имя теста "
        "-> строка лога -> стадия» делается по образцу; суждение о причине - нет. И вторая "
        "сторона, ваша как интегратора: мера экономии проверяется только строкой «Модели "
        "агентов» в отчётах - без неё она в следующий раз делается заново на ощупь."),
    "nova-p283": (
        "Окно остановлено словом владельца и работы не берёт - напоминание к сведению, на случай "
        "возобновления."),
}


def load_verdict():
    """The verdict tool as a module: ONE home for "which tree is this session".

    It already derives that with a mention threshold plus a cwd fallback. Asking a second way here
    would make two measurers of one subject, and the rule bought at 11:43Z says a disagreement
    between two of my own measurers is a defect, not noise.
    """
    path = os.path.join(ROOT, "scripts", "tools", "controller-verdict.py")
    spec = importlib.util.spec_from_file_location("verdict", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def stamp_path():
    return os.environ.get(STAMP_ENV) or DEFAULT_STAMP


def read_stamp():
    """Last send time, or None with a reason. An unreadable stamp counts as ABSENT, out loud.

    Not silently: a broken stamp dressed as "the hour has not passed" would suppress the reminder
    forever and the failure would look like ordinary quiet -- exactly the shape this role hunts.
    Absent means "send", which errs toward the owner's ceiling rather than past it.
    """
    p = stamp_path()
    try:
        with open(p, "r", encoding="utf-8", errors="replace") as fh:
            raw = fh.read().strip()
    except OSError:
        return None, "no stamp file at %s -- treated as absent, so: send" % p
    if not raw:
        return None, "stamp file empty -- treated as absent, so: send"
    try:
        return _dt.datetime.strptime(raw[:19], "%Y-%m-%dT%H:%M:%S"), None
    except ValueError:
        return None, "stamp UNPARSEABLE (%r) -- treated as absent, NOT as fresh" % raw[:40]


def write_stamp(now):
    p = stamp_path()
    d = os.path.dirname(p)
    if d:
        os.makedirs(d, exist_ok=True)
    with open(p, "w", encoding="utf-8") as fh:
        fh.write(now.strftime("%Y-%m-%dT%H:%M:%SZ") + "\n")
    return p


def peers(V):
    """Live peer sessions as (id8, tree). Same scan and same tree_of the verdict uses."""
    files = glob.glob(os.path.join(V.CFG, "projects", "*", "*.jsonl"))
    files = [f for f in files if os.path.basename(f) != V.ME + ".jsonl"]
    files.sort(key=os.path.getmtime, reverse=True)
    out = []
    for f in files[:4]:
        tail = V.SCAN.parse(V.SCAN.read_tail_lines(f, V.TAIL_BYTES))
        if not tail:
            continue
        cwd = next((e["cwd"] for e in tail if e.get("cwd")), None)
        sid = os.path.basename(f)[:-6]
        # THE DECLARED MAP FIRST, inference only as the fallback -- fixed 16:15Z 2026-09-09.
        # The map landed in the verdict at 15:37Z and this tool kept calling the OLD text-based
        # attribution, so the hourly letters named the main copy as the sdl window's tree while
        # the map (committed an hour earlier) knew `nova-sdl`. Third carrier today of one class:
        # a fix reaches the file being discussed and not the neighbour that shares the question.
        # The cure is not attention -- it is asking the ONE source both tools now agree on.
        decl = V.declared_trees(sid) if hasattr(V, "declared_trees") else []
        if decl:
            tree = decl[0]
        else:
            t, _ = V.tree_of(f, cwd)
            tree = os.path.basename(t or "?")
        out.append((sid[:8], os.path.basename(tree or "?")))
    return out


def main():
    argv = sys.argv[1:]
    force = "--force" in argv
    now = _dt.datetime.now(_dt.timezone.utc).replace(tzinfo=None)

    # The console here is cp1251 and the letters are Russian: printed straight, they came out as
    # mojibake -- a tool producing an answer that LOOKS like an answer and cannot be used. Found on
    # the first live run, 13:07Z 2026-09-09, which is the same class the whole role hunts. Two
    # fixes, both needed: re-encode stdout, and (below) write each letter to a file, because the
    # window reads a file with a known encoding while a terminal's encoding is whatever it is.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    except Exception:  # noqa: BLE001
        pass

    if "--sent" in argv:
        print("stamp written: %s -> %s" % (now.strftime("%H:%M:%SZ"), write_stamp(now)))
        return 0

    print("now_utc=%s now_local=%s  root=%s" % (
        now.strftime("%H:%M:%SZ"), _dt.datetime.now().strftime("%H:%M:%S"), ROOT))
    last, note = read_stamp()
    if note:
        print("stamp: %s" % note)
    else:
        age = (now - last).total_seconds()
        print("stamp: last sent %s, age %.1f min (interval %.0f min)" % (
            last.strftime("%H:%M:%SZ"), age / 60.0, INTERVAL_SEC / 60.0))
        if age < INTERVAL_SEC and not force:
            print("DECISION: NOT YET -- next send in %.1f min (or --force)" % (
                (INTERVAL_SEC - age) / 60.0))
            return 0
    print("DECISION: SEND NOW")

    try:
        rows = peers(load_verdict())
    except Exception as e:  # noqa: BLE001
        # Unknown recipients is an UNHANDLED CASE, not an answer: guessing names would send the
        # letter to nobody or to the wrong window (prohibition 16, applied to windows).
        print("recipients UNKNOWN: %s -- do not guess them, fix the scan first" % e)
        return 1
    if not rows:
        print("recipients: NONE alive -- nothing to send, stamp NOT written")
        return 1

    print("recipients: %s" % ", ".join("%s (%s)" % r for r in rows))
    out_dir = os.path.join(STATE_DIR, "reminders")
    os.makedirs(out_dir, exist_ok=True)
    stamp_hhmm = now.strftime("%H%M") + "Z"
    print()
    for sid, tree in rows:
        extra = PER_TREE.get(tree)
        body = ["%sZ КОНТРОЛЁР, напоминание владельца (не реже раза в час): %s"
                % (now.strftime("%H:%M"), COMMON)]
        if extra:
            body.append("")
            body.append("ЧТО ЭТО МЕНЯЕТ ДЛЯ ВАС ИМЕННО СЕЙЧАС: %s" % extra)
        body.append("")
        body.append("ПРОШУ: ничего, ответа не жду.")
        text = "\n".join(body) + "\n"
        path = os.path.join(out_dir, "%s-%s-%s.txt" % (stamp_hhmm, tree, sid))
        # utf-8 explicitly: the file is read back by the window, and a letter that arrives as
        # mojibake is worse than no letter -- it looks delivered.
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        note = "" if extra else "  [NO per-window part for this tree -- write it or skip]"
        print("%-11s %-8s -> %s%s" % (tree, sid, path, note))
    print()
    print("SEND: read each file and pass its content verbatim; the per-window part is the point.")
    print("AFTER SENDING: run with --sent, otherwise the next cycle sends again.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
