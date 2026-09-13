# -*- coding: utf-8 -*-
"""scripts/guards/check-gate-guard-dispatcher.py — шаг гейта не зовёт стража
именем, которого нет.

План: docs/plans/221.1-bug-sweep.md, строка №TBD (реестр 221.1).

ЧТО ЛОВИТ. Строка гейта, называющая файл стража
(`scripts/guards/<имя>.sh|.py`), обязана звать его ИМЕНЕМ, которое в этом же
файле определено функцией, либо одним из интерпретаторов. Имя, не
определённое нигде, — предмет стража.

ЗАЧЕМ, И ЭТО ЗАМЕР НА СЕБЕ (2026-09-13). Я завёл шаг для строки реестра
№1073 и написал вызов через `run_guard`. В гейте помощник зовётся `guard`;
`run_guard` не существует. Дальше произошло ровно то, ради чего этот страж:

    /d/Sources/nv-lang/nova/scripts/gate.sh: line 1787: run_guard: command not found
    GATE FAIL: данные в позиции C-формата ... (№1073)

Оболочка не нашла команду, вернула ненулевой код, и плечо `|| fail "..."`
напечатало сообщение О ПРЕДМЕТЕ. То есть шаг со СЛОМАННОЙ ПРОВОДКОЙ
неотличим от шага, НАШЕДШЕГО дефект. Читающий вердикт идёт чинить эмиттер,
которого никто не проверял.

ПОЧЕМУ НЕ ЛОВИТСЯ ТЕМ, ЧТО УЖЕ ЕСТЬ. У помощника `guard` есть защита №645
(«вышел с нулём, но не напечатал строку ok:» — ноль без строки не есть
проверка). Она не срабатывает никогда: до `guard` управление не доходит,
вызова с таким именем просто нет. `bash -n` тоже молчит — неизвестная
команда есть факт ИСПОЛНЕНИЯ, а не синтаксиса.

ПОЧЕМУ ПРАВИЛО ИМЕННО ТАКОЕ, А НЕ «ОБЯЗАН БЫТЬ `guard`». Замер по двум
гейтам: `scripts/gate.sh` зовёт 101 раз через `guard` и один раз напрямую
`bash` (захват вывода в переменную, не шаг-вердикт), а `scripts/gate-novac.sh`
диспетчеризует иначе — 87 раз `par_add`, 8 раз `guard`, один раз `python`.
Правило «обязан быть `guard`» покраснело бы на 91 здоровой строке novac-гейта
и было бы снято в тот же день. Проверяемое свойство — РАЗРЕШИМОСТЬ имени, и
она одинакова для обоих.

PATH НЕ СПРАШИВАЕТСЯ НАМЕРЕННО. Первый набросок разрешал имя через
`shutil.which`, и тогда вердикт зависел бы от того, зовётся ли на машине
`python` или `python3`: страж мерил бы ОКРУЖЕНИЕ, а предмет у него —
ТЕКСТ. Набор интерпретаторов зафиксирован ниже списком.

НУЛЕВАЯ ТЕРПИМОСТЬ, И ОНА ИЗМЕРЕНА: 198 строк-предметов в двух гейтах,
неразрешённых НОЛЬ. База не нужна — страж кусает только новое.

`$1` — корень. `$2` — override списка файлов (шов самотеста): каталог,
все `*.sh` внутри которого судятся как гейты.
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-gate-guard-dispatcher"

PATH_TOK = re.compile(r"scripts/guards/[A-Za-z0-9._-]+\.(?:sh|py)")
IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_.-]*$")
ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
FUNC_DEF = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*\(\)\s*\{", re.M)

# Слова оболочки, которые командой не являются: между ними и путём может
# стоять условие, и диспетчер стоит ещё левее.
KEYWORDS = {
    "if", "then", "else", "elif", "fi", "do", "done", "while", "until",
    "for", "case", "esac", "in", "return", "exit", "local", "time", "not",
}

# Интерпретаторы и утилиты, законно называющие файл стража напрямую.
# Список ФИКСИРОВАН: см. «PATH не спрашивается намеренно» в шапке.
RUNNERS = {
    "bash", "sh", "python", "python3", "env", "exec",
    "grep", "find", "sed", "awk", "cat", "echo", "printf", "test", "ls",
}


def dispatcher(line):
    """Имя, которым строка зовёт стража: ближайшее голое имя ЛЕВЕЕ пути."""
    toks = line.split()
    for k, tok in enumerate(toks):
        if not PATH_TOK.search(tok):
            continue
        for j in range(k - 1, -1, -1):
            w = toks[j].strip("\"'")
            if w.startswith("-") or not IDENT.match(w) or w in KEYWORDS:
                continue
            return w
        return None
    return None


def offenders(text):
    """(строка, имя) для каждого неразрешимого диспетчера."""
    funcs = set(FUNC_DEF.findall(text))
    out = []
    seen = 0
    for i, line in enumerate(text.split("\n"), 1):
        t = line.strip()
        if not t or t.startswith("#") or not PATH_TOK.search(t):
            continue
        if ASSIGN.match(t):          # присваивание пути в переменную — не вызов
            continue
        seen += 1
        w = dispatcher(t)
        if w is not None and (w in funcs or w in RUNNERS):
            continue
        out.append((i, w, " ".join(t.split())[:120]))
    return seen, out


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()

    if len(a) > 2:
        d = pathlib.Path(a[2])
        if not d.is_dir():
            print("%s ok: судить нечего (нет %s)" % (NAME, d))
            return 0
        files = sorted(d.glob("*.sh"))
    else:
        files = [p for p in (root / "scripts" / "gate.sh",
                             root / "scripts" / "gate-novac.sh") if p.is_file()]

    if not files:
        print("%s ok: судить нечего (гейт-скриптов не найдено)" % NAME)
        return 0

    bad = []
    subjects = 0
    for p in files:
        try:
            text = p.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        seen, rows = offenders(text)
        subjects += seen
        rel = p.relative_to(root) if str(p).startswith(str(root)) else p
        for i, w, line in rows:
            bad.append((str(rel).replace("\\", "/"), i, w, line))

    if not bad:
        print("%s ok: гейтов %d, строк-предметов %d, неразрешённых имён 0"
              % (NAME, len(files), subjects))
        return 0

    sys.stderr.write("%s: FAIL — стража зовут именем, которого нет: %d\n"
                     % (NAME, len(bad)))
    for rel, i, w, line in bad[:15]:
        sys.stderr.write("    %s:%d  имя: %s\n        %s\n"
                         % (rel, i, w if w else "(не найдено)", line))
    if len(bad) > 15:
        sys.stderr.write("    ... и ещё %d\n" % (len(bad) - 15))
    sys.stderr.write(
        "\n    Такой вызов оболочка не найдёт, вернёт ненулевой код, и плечо\n"
        "    `|| fail \"...\"` напечатает сообщение О ПРЕДМЕТЕ шага: сломанная\n"
        "    проводка станет неотличима от найденного дефекта.\n"
        "    Законных способов два: имя функции, ОПРЕДЕЛЁННОЙ в этом же файле\n"
        "    (`guard`, `par_add`), либо интерпретатор из фиксированного списка.\n"
        "    Замер 2026-09-13: `run_guard` вместо `guard` дал `command not\n"
        "    found` и вердикт о данных в C-формате, которых никто не проверял.\n")
    return 1


if __name__ == "__main__":
    sys.exit(main())
