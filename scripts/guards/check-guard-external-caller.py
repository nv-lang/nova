# -*- coding: utf-8 -*-
"""scripts/guards/check-guard-external-caller.py — у стража есть ВНЕШНИЙ
вызывающий (конвенция гейтов, Г8).

ЗАЧЕМ. Механизм, который запускает только тот, кто о нём помнит, — не механизм.
Внешний вызывающий — это тот, кого нельзя забыть: CI или git-хук. Гейт, который
надо помнить запустить, таковым не является, и его собственный документ это
уже говорит про время: гейт на девять минут не гоняют.

ЧТО СЧИТАЕТСЯ ВНЕШНИМ ВЫЗЫВАЮЩИМ:
  * рабочий поток CI — прямым вызовом стража;
  * гейт, который CI ЗАПУСКАЕТ (не просто упоминает: первая проба этого правила
    сама попалась на комментарий «they used to live in scripts/gate.sh» и
    ответила «зовёт» — Г9 в чистом виде, поэтому комментарии здесь не считаются);
  * git-хук.

ХРАПОВИК, а не запрет. На день заведения (2026-08-19) без внешнего вызывающего
48 стражей из 120: 45 названы только в `scripts/gate.sh`, который CI не
запускает, и три не зовёт никто. Запретить это одним днём значило бы либо
красный гейт, либо перенос сорока восьми проверок в CI без разбора — а разбирать
надо каждую. Поэтому число записано базой и может ТОЛЬКО УБЫВАТЬ.

ПОЧЕМУ ЭТО НЕ ДУБЛИКАТ check-guard-wiring. Тот спрашивает «подключён ли страж к
gate.sh и есть ли самотест»; этот — «а сам gate.sh кто-нибудь запускает?».
Разные вопросы: страж может быть образцово подключён к гейту, который никто не
гоняет.

$1 — корень репозитория; $2 — override базы (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-guard-external-caller"
GUARD_RE = re.compile(r"check-[a-z0-9-]+\.(?:sh|py)")
RE_BASE = re.compile(r"^without-external-caller[^\S\n]+(\d+)")


def code_of(path):
    """Текст БЕЗ комментариев: упоминание в комментарии вызовом не является."""
    out = []
    for line in path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        s = line.lstrip()
        if s.startswith("#"):
            continue
        out.append(line.split(" #")[0] if path.suffix in (".yml", ".yaml") else line)
    return "\n".join(out)


def named_in(paths):
    out = set()
    for p in paths:
        if p.is_file():
            out |= set(GUARD_RE.findall(code_of(p)))
    return out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    base_file = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "guards" / "guard-callers.baseline"
    gdir = root / "scripts" / "guards"

    if not gdir.is_dir():
        print(f"{NAME} ok: судить нечего (нет {gdir})")
        return 0

    guards = sorted({p.name for p in gdir.glob("check-*.sh")} | {p.name for p in gdir.glob("check-*.py")})
    if not guards:
        print(f"{NAME}: FAIL — в {gdir} нет ни одного стража: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    wf_dir = root / ".github" / "workflows"
    workflows = sorted(wf_dir.glob("*.yml")) if wf_dir.is_dir() else []
    ci_code = "\n".join(code_of(p) for p in workflows)

    hooks_dir = root / "scripts" / "githooks"
    hooks = sorted(p for p in hooks_dir.glob("*") if p.is_file()) if hooks_dir.is_dir() else []

    external = named_in(workflows) | named_in(hooks)
    # Гейт засчитывается ТОЛЬКО если CI его запускает.
    gates_run_by_ci = []
    for gate in sorted((root / "scripts").glob("gate*.sh")):
        if re.search(r"(bash|sh)\s+\S*" + re.escape(gate.name), ci_code):
            gates_run_by_ci.append(gate)
            external |= named_in([gate])

    orphans = [g for g in guards if g not in external]

    if not base_file.is_file():
        print(f"{NAME}: FAIL — нет базы {base_file}: судить нечем, а нечем != зелено", file=sys.stderr)
        return 1
    base = None
    for line in base_file.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
        m = RE_BASE.match(line)
        if m:
            base = int(m.group(1))
            break
    if base is None:
        print(f"{NAME}: FAIL — в {base_file} нет строки `without-external-caller <число>`",
              file=sys.stderr)
        return 1

    n = len(orphans)
    gates = ", ".join(g.name for g in gates_run_by_ci) if gates_run_by_ci else "ни одного"
    if n > base:
        print(f"{NAME}: FAIL — стражей без внешнего вызывающего {n}, в базе {base} — РОСТ (Г8):",
              file=sys.stderr)
        for g in orphans[:15]:
            print(f"  {g}", file=sys.stderr)
        if n > 15:
            print(f"  ... и ещё {n - 15}", file=sys.stderr)
        print(f"  Внешний вызывающий — CI или git-хук. Гейты, которые запускает CI: {gates}.",
              file=sys.stderr)
        print("  Страж, которого зовёт только гейт «по памяти», держится на том, что кто-то", file=sys.stderr)
        print("  помнит его запустить, — а помнить перестают ровно тогда, когда гейт долгий.", file=sys.stderr)
        return 1
    if n < base:
        print(f"{NAME}: FAIL — стражей без внешнего вызывающего {n}, в базе {base} — "
              f"ПРОГРЕСС без опускания базы", file=sys.stderr)
        print(f"  Опусти число в {base_file} ТЕМ ЖЕ коммитом: иначе следующий рост до прежней",
              file=sys.stderr)
        print("  цифры пройдёт молча.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: стражей {len(guards)}, под внешним судьёй {len(guards) - n}, "
          f"без внешнего вызывающего {n} (== база); гейты под CI: {gates}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
