# -*- coding: utf-8 -*-
"""scripts/guards/check-guard-honesty.py — страж не может соврать или промолчать
вместо проверки.

Адрес: план 274.3, находка F119 — там записано, как этот страж судил только
`*.sh` и каждый порт на `*.py` выходил из-под суда.
Ссылка проставлена 2026-08-27 по требованию `check-guard-wiring` (реестр 221.1 №785).

ЗАЧЕМ. Вердикт без проверки хуже отсутствия стража: по нему принимают решения.
Каждое правило ниже заведено ПО СЛУЧАЮ, а не из общих соображений.

ПРОВЕРЯЕТ scripts/**/*.sh:
  * ни одного CRLF в ИНДЕКСЕ (рабочее дерево на Windows законно иное): под `sh`
    на Linux такой скрипт не запускается, а гейт печатает его крах как
    нарушение правила;
  * файл, называющий `nova.exe`, знает и второе имя — через дверь
    `novac_find_oracle` или прямым запасным путём: иначе на Linux он не найдёт
    оракула и ПРОМОЛЧИТ;
  * апостроф в двойных кавычках у echo/printf: оболочка выполнит его как
    команду, символ пропадёт из текста, а в вывод упадёт «x: command not
    found» — и видно это только на КРАСНОЙ ветке, то есть ровно тогда, когда
    сообщение читают;
  * СЪЕДЕННЫЙ возврат каретки: `tr -d '<перевод строки>'` — кавычка открыта в
    конце строки и закрыта в начале следующей. Вместо `\\r` сносятся ПЕРЕВОДЫ
    СТРОК, файл склеивается в ОДНУ строку, и всякий последующий grep находит в
    ней что угодно. Поймано 2026-08-19 в check-novac-row-fields.sh: правило П23
    полтора дня засчитывало пометку с ЧУЖОЙ строки плана и пропустило живое
    нарушение. НЕ судится замена `tr '<перевод строки>' ' '`: склеить список в
    одну строку — законная идиома.

ПРОВЕРЯЕТ scripts/**/*.py — стражи переезжают на python (П14: один старт
интерпретатора вместо сорока), и судья, который смотрит только .sh, слепнет
РОВНО ПО МЕРЕ ПЕРЕЕЗДА (класс №519):
  * та же слепота к имени бинаря;
  * `check-*.py`, который печатает вердикт, не переведя поток на LF: python на
    Windows пишет CRLF там, где shell писал LF, и вывод расходится с
    shell-редакцией молча — сверка порта перестаёт что-либо значить.

ПРОВЕРЯЕТ дерево целиком: ДВОЙНОЙ возврат каретки (`\\r\\r`) в отслеживаемых
файлах. 2026-08-19 патч-скрипт записал novac/src/parse/decls.nv с `\\r\\r\\n` —
81 строка; компилятор стерпел, а всякий читатель с универсальными переводами
строк увидел ВДВОЕ больше строк, и номера в диагностиках уехали на девять.

НЕ ПРОВЕРЯЕТ: строки, содержащие одинарную кавычку, — там живут awk-программы и
фикстуры самотестов, где апостроф безобиден (сознательная слепая зона, названная
здесь, а не молчаливая); смысл сообщений; прочие башизмы — их судит сам CI, и
это честнее, чем угадывать их список.

ОТДЕЛЬНОЙ ветки «сканер упал» здесь нет, и это не потеря: в shell скан был
процессом awk, чей крах давал пустой результат, неотличимый от «находок нет»
(2026-08-19 такой страж напечатал «ok»). Здесь разбор идёт в самом страже —
поломка поднимает исключение и убивает процесс ненулевым кодом. Границу держит
случай 13 самотеста.

ПОЧЕМУ PYTHON: shell-редакция поднимала два awk и два `git ... | xargs` — 2.1с,
из которых работой были два вызова git (П14).

$1 — корень репозитория.
"""
import os
import pathlib
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-guard-honesty"
DOOR = "lib/novac.sh"

RE_EXE = re.compile(r"release/nova\.exe")
RE_COMMENT = re.compile(r"^[ \t\v\f]*#")
RE_FALLBACK_SH = re.compile(r'release/nova"|release/nova ')
RE_FALLBACK_PY = re.compile(r"release/nova[^.]|release/nova$")
RE_SAYS = re.compile(r"^[ \t\v\f]*(echo|printf|ok|bad)[ \t\v\f]")
RE_EATEN = re.compile(r"tr[ \t\v\f]+-d[ \t\v\f]*'$")
RE_PRINT = re.compile(r"(^|[^A-Za-z_.])print\(")
RE_CHECK_PY = re.compile(r"check-[^/]*\.py$")


def read_lines(path):
    out = path.read_bytes().decode("utf-8", "replace").split("\n")
    if out and out[-1] == "":
        out.pop()
    return [l[:-1] if l.endswith("\r") else l for l in out]


def git(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], capture_output=True).stdout


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else
                        pathlib.Path(__file__).resolve().parents[2]).resolve()
    d = root / "scripts"

    if not d.is_dir():
        print(f"{NAME} ok: судить нечего (нет {d})")
        return 0

    sh_files, py_files = [], []
    for dirpath, _dirs, names in os.walk(d):
        for nm in names:
            p = pathlib.Path(dirpath) / nm
            if nm.endswith(".sh"):
                sh_files.append(p)
            elif nm.endswith(".py"):
                py_files.append(p)
    sh_files.sort(key=lambda p: str(p).replace("\\", "/"))
    py_files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not sh_files:
        print(f"{NAME}: FAIL — в {d} нет ни одного .sh: судить нечего там, где судить обязано",
              file=sys.stderr)
        return 1

    bad = []

    # (1) CRLF — в ИНДЕКСЕ, а не в рабочем дереве -----------------------------
    eol = git(root, "ls-files", "--eol", "--", "scripts/*.sh", "scripts/**/*.sh")
    for line in eol.decode("utf-8", "replace").split("\n"):
        f = line.split()
        if len(f) >= 2 and "i/crlf" in f[0]:
            bad.append(f"  {f[-1]}: CRLF в индексе — под sh на Linux не запустится")

    # (2,3,6) — разбор shell-скриптов ---------------------------------------
    def rel_of(p):
        return str(p).replace("\\", "/").replace(str(root).replace("\\", "/") + "/", "", 1)

    for p in sh_files:
        rel = rel_of(p)
        skip = DOOR in rel or "/selftest/" in rel
        saw_exe = saw_door = saw_fallback = False
        prev = ""
        for n, line in enumerate(read_lines(p), 1):
            if not skip:
                if not RE_COMMENT.match(line) and RE_EXE.search(line):
                    saw_exe = True
                if "novac_find_oracle" in line:
                    saw_door = True
                if RE_FALLBACK_SH.search(line):
                    saw_fallback = True
            if (RE_SAYS.match(line) and "'" not in line
                    and "\\`" not in line and "`" in line):
                bad.append(f"  {rel}:{n}: апостроф в двойных кавычках — "
                           f"оболочка выполнит его как команду")
            if RE_EATEN.search(prev) and line.startswith("'"):
                bad.append(f"  {rel}:{n - 1}: перевод строки в одинарных кавычках — "
                           f"съеденный \\r: снесёт строки вместо возвратов каретки, "
                           f"и правило умрёт молча")
            prev = line
        if not skip and saw_exe and not saw_door and not saw_fallback:
            bad.append(f"  {rel}: знает только nova.exe — на Linux не найдёт оракула и промолчит")

    # (4) ДВОЙНОЙ возврат каретки в отслеживаемых файлах ---------------------
    tracked = git(root, "ls-files", "-z", "--", "novac/**", "scripts/**", "docs/**")
    for name in tracked.split(b"\0"):
        if not name:
            continue
        f = root / name.decode("utf-8", "replace")
        try:
            data = f.read_bytes()
        except OSError:
            continue
        if b"\0" in data:          # grep -I: бинарные файлы не судятся
            continue
        if b"\r\r" in data:
            bad.append(f"  {name.decode('utf-8', 'replace')}: двойной возврат каретки "
                       f"(\\r\\r) — файл записан сломанным инструментом")

    # (5) Питоновские стражи — под тем же судом ------------------------------
    for p in py_files:
        rel = rel_of(p)
        if "/selftest/" in rel:
            continue
        is_check = bool(RE_CHECK_PY.search(rel))
        saw_exe = saw_fallback = saw_print = saw_lf = False
        for line in read_lines(p):
            if not RE_COMMENT.match(line) and RE_EXE.search(line):
                saw_exe = True
            if RE_FALLBACK_PY.search(line) or "novac_find_oracle" in line:
                saw_fallback = True
            if RE_PRINT.search(line):
                saw_print = True
            if "reconfigure(" in line and "newline=" in line:
                saw_lf = True
        if saw_exe and not saw_fallback:
            bad.append(f"  {rel}: знает только nova.exe — на Linux не найдёт оракула и промолчит")
        if is_check and saw_print and not saw_lf:
            bad.append(f"  {rel}: печатает вердикт, не переведя поток на LF — на Windows это CRLF")

    if bad:
        print(f"{NAME}: FAIL — страж может соврать или промолчать вместо проверки", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Вердикт без проверки хуже отсутствия стража: по нему принимают решения.",
              file=sys.stderr)
        return 1

    print(f"{NAME} ok: скриптов проверено {len(sh_files)} (.sh) и {len(py_files)} (.py), "
          f"CRLF 0, слепых к имени бинаря 0, съедаемых оболочкой сообщений 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
