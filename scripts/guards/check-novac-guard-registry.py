# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-guard-registry.py — МЕТА-СТРАЖ реестра стражей
novac: план, файлы, вызовы в гейте и самотесты сходятся.

ЧЕТЫРЕ МНОЖЕСТВА и что означает расхождение:
  A — имена в §10.3/§10.3а плана 274 (аудит);
  B — файлы `scripts/guards/check-novac-*`;
  C — вызовы в `scripts/gate.sh` и `scripts/gate-novac.sh`;
  D — самотесты `scripts/guards/selftest/test-check-novac-*`.
Файл без вызова — ложное спокойствие; файл без самотеста — страж, чью краснотУ
никто не доказал; имя в плане без файла — правило на бумаге (законно ТОЛЬКО с
маркером часов «ждёт этапа»); файл вне аудита — правило, невидимое приёмке.

ИМЯ СРАВНИВАЕТСЯ БЕЗ РАСШИРЕНИЯ (2026-08-19). Пока стражи были только `.sh`,
множества строились по литералу `check-novac-*.sh`, и первый же перевод стража
на python сделал его НЕВИДИМЫМ разом во всех четырёх: имён в плане стало 53
вместо 64, а вызовы, самотесты и записи аудита перестали проверяться — то есть
мета-страж молча перестал стеречь двенадцать стражей. Расширение — деталь
реализации правила, а не его идентичность.

ПОЧЕМУ PYTHON: shell-редакция стоила 29с — рекорд среди стражей; она поднимала
grep/sed/comm десятки раз и basename на каждый файл.

$1 — корень репозитория.
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-guard-registry"
CLOCK = "\U0001F550"
GUARD_RE = re.compile(r"check-novac-[a-z0-9-]+\.(?:sh|py)")


def base(n):
    """Имя без расширения: идентичность стража — правило, а не язык."""
    return re.sub(r"\.(sh|py)$", "", n)


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    plan = root / "docs/plans/274-novac-self-hosted-compiler.md"
    guards = root / "scripts/guards"
    self_dir = guards / "selftest"
    gates = [p for p in (root / "scripts/gate.sh", root / "scripts/gate-novac.sh") if p.is_file()]

    if not plan.is_file():
        print(f"{NAME}: FAIL — нет плана {plan}", file=sys.stderr)
        return 1
    if not gates:
        print(f"{NAME}: FAIL - не найдено ни одного гейта (ни scripts/gate.sh, ни scripts/gate-novac.sh)", file=sys.stderr)
        return 1

    text = plan.read_text(encoding="utf-8", errors="replace").replace("\r", "")
    lines = text.split("\n")

    heads = sum(1 for l in lines if l.startswith("### 10.3"))
    if heads < 2:
        print(f"{NAME}: FAIL — в плане найдено {heads} заголовков '### 10.3*', нужно минимум 2", file=sys.stderr)
        print("  Разделы переименованы или удалены — реестр стражей судить нечем.", file=sys.stderr)
        return 1
    if not any(l.startswith("### 10.4.") for l in lines):
        print(f"{NAME}: FAIL — в плане нет заголовка '### 10.4.' — диапазон §10.3 не ограничен", file=sys.stderr)
        return 1

    # --- A: имена из §10.3..§10.4, вне фенсед-блоков ---------------------
    rows, inside, fence = [], False, False
    for l in lines:
        if l.startswith("### 10.3."):
            inside = True
            continue
        if inside and l.startswith("### 10.4."):
            break
        if not inside:
            continue
        if l.startswith("```"):
            fence = not fence
            continue
        if fence:
            continue
        if l.startswith("|"):
            rows.append(l)

    if not rows:
        print(f"{NAME}: FAIL — таблицы §10.3/§10.3а пусты или не разобраны", file=sys.stderr)
        print("  Проверь разметку: строки таблиц обязаны начинаться с '|'.", file=sys.stderr)
        return 1

    a, a_clock = set(), set()
    shown = {}          # base -> как это имя пишется (для сообщений)
    for row in rows:
        for n in GUARD_RE.findall(row):
            shown.setdefault(base(n), n)
        names = {base(n) for n in GUARD_RE.findall(row)}
        a |= names
        if CLOCK in row:
            a_clock |= names

    # --- B: файлы ---------------------------------------------------------
    b = set()
    for f in guards.glob("check-novac-*"):
        if f.is_file() and f.suffix in (".sh", ".py"):
            b.add(base(f.name))
            shown[base(f.name)] = f.name      # файл на диске главнее плана

    # --- C: вызовы в гейтах ----------------------------------------------
    c = set()
    for g in gates:
        for l in g.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"):
            if l.lstrip().startswith("#"):
                continue
            if not re.search(r"(^|;\s*then\s+)\s*(guard|par_add)\s", l):
                continue
            c |= {base(n) for n in GUARD_RE.findall(re.sub(r"\|\|.*$", "", l))}

    # --- D: самотесты -----------------------------------------------------
    d = set()
    if self_dir.is_dir():
        for p in self_dir.glob("test-check-novac-*"):
            if p.is_file() and p.suffix in (".sh", ".py"):
                d.add(base(p.name[len("test-"):]))

    v_gate = sorted(b - c)
    v_self = sorted(b - d)
    a_nofile = a - b
    a_pending = sorted(a_nofile & a_clock)
    v_plan = sorted(a_nofile - a_clock)
    v_audit = sorted(b - a)
    d_orphan = sorted(d - b)

    bad = len(v_gate) + len(v_self) + len(v_plan) + len(v_audit)
    if bad:
        print(f"{NAME}: FAIL — реестр стражей novac разошёлся с планом/гейтом/самотестами", file=sys.stderr)
        print(f"  множества: имён в плане {len(a)}, файлов {len(b)}, вызовов в гейте {len(c)}, самотестов {len(d)}", file=sys.stderr)
        if v_gate:
            print("", file=sys.stderr)
            print("1) СТРАЖ ЕСТЬ, НО НЕ ВЫЗВАН В scripts/gate.sh:", file=sys.stderr)
            for n in v_gate:
                print(f"     {shown.get(n, n)}", file=sys.stderr)
            print("   Как чинить: добавить в гейт шаг вида", file=sys.stderr)
            print('     guard "$ROOT/scripts/guards/<имя>" "$ROOT" || fail "<что нарушено>"', file=sys.stderr)
            print("   Незапускаемый страж хуже отсутствия: даёт ложное спокойствие.", file=sys.stderr)
        if v_self:
            print("", file=sys.stderr)
            print("2) СТРАЖ ЕСТЬ, НО НЕТ САМОТЕСТА:", file=sys.stderr)
            for n in v_self:
                print(f"     {shown.get(n, n)}", file=sys.stderr)
            print("   Как чинить: завести scripts/guards/selftest/test-<имя>.sh на ОБА исхода", file=sys.stderr)
            print("   (зелёный случай И красный на подложке во временном каталоге).", file=sys.stderr)
        if v_plan:
            print("", file=sys.stderr)
            print("3) ИМЯ НАЗВАНО ПЛАНОМ, ФАЙЛА НЕТ, МАРКЕРА ЭТАПА НЕТ:", file=sys.stderr)
            for n in v_plan:
                print(f"     {shown.get(n, n)}", file=sys.stderr)
            print("   Как чинить: либо завести страж файлом, либо поставить в строке плана", file=sys.stderr)
            print("   маркер часов ('заводится на названном этапе'), либо убрать строку.", file=sys.stderr)
        if v_audit:
            print("", file=sys.stderr)
            print("4) СТРАЖ ЕСТЬ, НО ЕГО НЕТ В АУДИТЕ §10.3/§10.3а:", file=sys.stderr)
            for n in v_audit:
                print(f"     {shown.get(n, n)}", file=sys.stderr)
            print("   Как чинить: внести строку в таблицу §10.3а плана 274.", file=sys.stderr)
            print("   Правило без записи в аудите невидимо приёмке.", file=sys.stderr)
        print("", file=sys.stderr)
        print("План: docs/plans/274-novac-self-hosted-compiler.md §10.3, §10.3а.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: имён в плане §10.3/§10.3а {len(a)} (ждут этапа под маркером {len(a_pending)}), "
          f"файлов стражей {len(b)}, вызовов в gate.sh {len(c)}, самотестов {len(d)} "
          f"(без своего стража {len(d_orphan)}), расхождений 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
