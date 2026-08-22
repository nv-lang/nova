# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-temp-edges.py — временные рёбра таблицы §3 и
временные ice-маркеры в коде САМОИСТЕКАЮТ (аудит механизмов 2026-08-16).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19). Временное,
не имеющее срока, вечно; поэтому каждое временное ребро §3 и каждый маркер
`user-error-as-ice` обязаны нести `until:<этап>` — этап, ДО которого они
законны. Текущий этап живёт машинной строкой `#   stage: <этап>` в
novac/nova.toml (та же дверь, что spec-point/oracle-pin). Наступил этап —
красный, пока временное не снято той же волной.

ПОЧЕМУ PYTHON, А НЕ SHELL. Замер 2026-08-19: shell-редакция стоила 3.5с на
дереве в 32 файла, и 99.8% этого — ЗАПУСК ПРОЦЕССОВ: она поднимала grep, sed и
cut на КАЖДУЮ строку обеих таблиц. Один процесс читает то же дерево и гоняет
десяток правил за 0.047с. Правило не изменилось ни на слово — изменилась цена
формы (П14: скорость первична, и гейт, который её стережёт, был самым медленным
в комнате).

ПРОВЕРЯЕТ:
  * строки таблицы §3 архитектуры со словом «временн»: у каждой есть
    `until:<этап>`, и этап ещё не наступил;
  * маркеры `user-error-as-ice` в novac/src/**/*.nv (тесты исключены): то же.
НЕ ПРОВЕРЯЕТ: осмысленность самого ребра (это ревью приёмки) и то, что этап в
nova.toml поставлен честно (его двигает сознательный коммит).

Аргументы: [корень] [путь к архитектуре] [путь к nova.toml] [путь к novac/src]
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-temp-edges"
ORDER = ["E1", "E2", "E2b1", "E2b2", "E2b3", "E3", "E4", "E5", "E6"]


def rank(stage):
    return ORDER.index(stage) + 1 if stage in ORDER else 0


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    arch = pathlib.Path(a[2]) if len(a) > 2 else root / "docs/dev/novac-architecture.md"
    toml = pathlib.Path(a[3]) if len(a) > 3 else root / "novac/nova.toml"
    src = pathlib.Path(a[4]) if len(a) > 4 else root / "novac/src"

    if not arch.is_file():
        print(f"{NAME} ok: судить нечего (нет {arch})")
        return 0
    if not toml.is_file():
        print(f"{NAME}: FAIL — нет {toml}: якоря этапа нет, временные рёбра судить нечем", file=sys.stderr)
        return 1

    m = re.search(r"^#   stage: ([A-Za-z0-9]+)$", toml.read_text(encoding="utf-8", errors="replace").replace("\r", ""), re.M)
    if not m:
        print(f"{NAME}: FAIL — в {toml} нет строки '#   stage: <этап>' (строгая форма)", file=sys.stderr)
        return 1
    stage = m.group(1)
    cur = rank(stage)
    if cur == 0:
        print(f"{NAME}: FAIL — этап '{stage}' неизвестен (законны: {' '.join(ORDER)})", file=sys.stderr)
        return 1

    # --- временные рёбра таблицы §3 --------------------------------------
    rows = []
    for i, line in enumerate(arch.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
        if line.startswith("|") and "временн" in line:
            rows.append((i, line))

    no_until, expired = [], []
    for i, line in rows:
        u = re.search(r"until:(E[0-9b]*)", line)
        if not u:
            no_until.append(f"{i}: {line}"[:90])
            continue
        r = rank(u.group(1))
        head = f"{i}: {line}"[:80]
        if r == 0:
            expired.append(f"  {head} — этап '{u.group(1)}' неизвестен")
        elif cur >= r:
            expired.append(f"  {head} — истекло: until:{u.group(1)}, а stage уже {stage}")

    if no_until:
        print(f"{NAME}: FAIL — временное ребро БЕЗ срока (274.1 §2в; аудит 2026-08-16):", file=sys.stderr)
        for l in no_until:
            print(f"  {l}", file=sys.stderr)
        print("  Временное обязано нести until:<этап> — иначе оно вечное.", file=sys.stderr)
        return 1
    if expired:
        print(f"{NAME}: FAIL — временное ребро ИСТЕКЛО:", file=sys.stderr)
        for l in expired:
            print(l, file=sys.stderr)
        print("  Этап наступил — сними ребро из таблицы §3 и из импортов,", file=sys.stderr)
        print("  либо сдвинь until: сознательным коммитом с причиной.", file=sys.stderr)
        return 1

    # --- маркеры в коде ---------------------------------------------------
    marks, mbad = 0, []
    if src.is_dir():
        for p in sorted(src.rglob("*.nv")):
            if p.name.endswith("_test.nv"):
                continue
            rel = p.relative_to(src).as_posix()
            for i, line in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
                if "user-error-as-ice" not in line:
                    continue
                marks += 1
                loc = f"{rel}:{i}"
                u = re.search(r"until:(E[0-9b]*)", line)
                if not u:
                    mbad.append(f"  {loc} — маркер user-error-as-ice БЕЗ until:<этап>")
                    continue
                r = rank(u.group(1))
                if r == 0:
                    mbad.append(f"  {loc} — этап '{u.group(1)}' неизвестен")
                elif cur >= r:
                    mbad.append(f"  {loc} — истёк: until:{u.group(1)}, а stage уже {stage} — чекер обязан был забрать это на себя")

    if mbad:
        print(f"{NAME}: FAIL — ice на ошибке пользователя пережил свой срок:", file=sys.stderr)
        for l in mbad:
            print(l, file=sys.stderr)
        print("  Ошибка пользователя — диагностика, не ice (нулевая терпимость);", file=sys.stderr)
        print("  это временно и ТОЛЬКО со сроком; наступил этап — перенеси в check.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: stage {stage}, временных рёбер {len(rows)}, все со сроком, истёкших 0; "
          f"ice-маркеров на ошибке пользователя {marks}, все со сроком")
    return 0


if __name__ == "__main__":
    sys.exit(main())
