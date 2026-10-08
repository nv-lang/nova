# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-selftest-interpreter.py — самотест зовёт стража
ЧЕРЕЗ интерпретатор, а не напрямую (класс «зелено на Windows, мертво на Linux»;
замер 2026-08-23).

ЗАЧЕМ. Стражи в этом дереве лежат с режимом 100644 — без бита исполнения, по
соглашению репозитория. На Windows это неважно: MSYS запускает файл по шебангу.
На Linux `"$G" ...` даёт «Permission denied» (rc=126), и самотест валится ВЕСЬ
— каждый случай печатает FAIL, ни одного `ok`.

Замер 2026-08-23 (по красному CI): два самотеста из 147 звали стража напрямую —
оба мои, оба вчерашние. На CI они напечатали НОЛЬ строк `ok`, и мой же страж
счётчиков реестра доложил «реестр обещает 12 случаев, самотест печатает 0» —
то есть красный пришёл не оттуда, где ошибка. Остальные 145 зовут `bash "$G"`
или `python "$G"` (460 мест) — конвенция была, соблюдалась молча и потому не
держалась ничем.

ПРАВИЛО ПЛОСКОЕ, база ноль: прямой вызов — это не наследство, а опечатка,
которая на машине автора не видна.

ВТОРОЕ ПРАВИЛО (2026-10-07, реестр 221.1 №1825): интерпретатор НЕ СПОРИТ С
ШЕБАНГОМ. `sh <скрипт>`, чей шебанг — bash, на Windows работает (MSYS `sh` —
это bash), а на Linux `sh` — это dash: `set -o pipefail`, `<(...)`, `[[` дают
синтаксическую ошибку, страж выходит с кодом 2, и самотест краснеет НЕ ТАМ, где
ошибка. Замер 2026-10-07: 50 таких вызовов в 12 файлах; два из них
(`test-check-runtime-cxx-clean`, `test-check-registry-single-verdict`) держали
ночной ярус full красным с 2026-10-01, остальные проходили по везению —
их стражи пока не пользовались башизмами. Первое правило считало `sh "$G"`
законной формой; второе уточняет: законной, пока цель — sh-скрипт. Судятся все
`scripts/**/*.sh` (вызов стража живёт и вне самотестов —
`check-commit-hygiene.sh`), цель узнаётся по литеральному имени `*.sh` в
аргументе или по присваиванию `ИМЯ=…/x.sh` выше в том же файле.

ЧЕГО НЕ ПРОВЕРЯЕТ: вызовы через переменную с другим именем (конвенция — `$G`,
названная слепая зона; во втором правиле — переменная без присваивания `…/x.sh`
в том же файле); башизмы в скриптах с шебангом `sh` (их найдёт только запуск под
dash — замер 2026-10-07 `dash -n` по 160 таким скриптам: синтаксис не держат
`gate.sh`, который зовут только `bash scripts/gate.sh`, и один самотест, который
гейт тоже зовёт через bash); строки, которые ПЕЧАТАЮТ прямой вызов (`printf`/`echo`/
`cat` пишут фикстуру — это данные, а не вызов; без исключения страж краснел на
собственном самотесте, поймано первым прогоном по дереву); прочие
платформозависимые инструменты внутри самотестов (`cygpath` и родня — отдельный
класс, план 274 §9.1д К3).

$1 — корень; $2 — override каталога самотестов (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-selftest-interpreter"

# «$G» в позиции КОМАНДЫ: начало строки, после $( или ` , после && / || / ; .
RE_DIRECT = re.compile(r'(?:^|\$\(|`|&&|\|\||;)\s*"\$G"')
RE_OK = re.compile(r'\b(?:bash|sh|python|python3)\s+"\$G"')
RE_WRITES = re.compile(r"(?:printf|echo|cat)\b")
# Второе правило: `sh <аргумент>` в позиции команды и как его цель узнаётся.
RE_SH = re.compile(r'(?:^|(?<=[\s;&|(`!]))sh\s+("?)(\$\{?[A-Za-z_]\w*\}?[^\s"]*|[^\s"$-][^\s"]*)\1')
RE_SH_NAME = re.compile(r"([\w.-]+\.sh)$")
RE_VAR = re.compile(r"\$\{?([A-Za-z_]\w*)\}?$")
RE_ASSIGN = re.compile(r"""^\s*(?:export\s+|local\s+)?([A-Za-z_]\w*)=["']?[^\s"']*?([\w.-]+\.sh)["']?\s*$""")


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    sel = pathlib.Path(a[2]) if len(a) > 2 else root / "scripts" / "guards" / "selftest"

    if not sel.is_dir():
        print(f"{NAME} ok: судить нечего (нет {sel})")
        return 0

    files = sorted(sel.glob("*.sh"))
    if not files:
        print(f"{NAME}: FAIL — в {sel} нет ни одного самотеста: страж потерял "
              f"мишень (класс №519)", file=sys.stderr)
        return 1

    bad = []
    for f in files:
        for n, line in enumerate(f.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            s = line.strip()
            if s.startswith("#"):
                continue
            # СТРОКА, КОТОРАЯ ПЕЧАТАЕТ, НЕ ЗОВЁТ. Самотесты пишут фикстуры
            # через printf/echo/cat, и текст фикстуры законно содержит прямой
            # вызов — иначе нельзя проверить, что страж его ловит. Поймано
            # первым же прогоном по дереву: страж покраснел на СВОЁМ самотесте.
            if RE_WRITES.match(s):
                continue
            if RE_DIRECT.search(line) and not RE_OK.search(line):
                bad.append(f"  {f.name}:{n}: {s[:72]}")

    # Второе правило: `sh` на скрипт с шебангом bash — по всему scripts/.
    scope = {p.resolve() for p in files}
    if (root / "scripts").is_dir():
        scope.update(p.resolve() for p in (root / "scripts").rglob("*.sh"))
    scope = sorted(scope)
    bash_names = {p.name for p in scope if is_bash(p)}
    mism = []
    calls = 0
    for f in scope:
        assigned = {}
        for n, line in enumerate(f.read_text(encoding="utf-8", errors="replace").split("\n"), 1):
            m = RE_ASSIGN.match(line)
            if m:
                assigned[m.group(1)] = m.group(2)
            s = line.strip()
            if s.startswith("#") or RE_WRITES.match(s):
                continue
            for c in RE_SH.finditer(line):
                arg = c.group(2)
                lit = RE_SH_NAME.search(arg)
                v = RE_VAR.match(arg)
                target = lit.group(1) if lit else (assigned.get(v.group(1)) if v else None)
                if target is None:
                    continue
                calls += 1
                if target in bash_names:
                    rel = str(f.relative_to(root)) if f.is_relative_to(root) else f.name
                    mism.append(f"  {rel.replace(chr(92), '/')}:{n}: sh -> {target} (шебанг bash)")

    if bad:
        print(f"{NAME}: FAIL — самотест зовёт стража напрямую (мест {len(bad)}):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Стражи лежат без бита исполнения (100644). На Windows файл", file=sys.stderr)
        print("  запускается по шебангу, на Linux это rc=126 — и самотест", file=sys.stderr)
        print("  печатает НОЛЬ строк `ok`, то есть красный приходит не оттуда,", file=sys.stderr)
        print("  где ошибка (замер 2026-08-23, красный CI). Зови через", file=sys.stderr)
        print("  интерпретатор: `bash \"$G\"` или `python \"$G\"`.", file=sys.stderr)
    if mism:
        print(f"{NAME}: FAIL — `sh` зовёт скрипт с шебангом bash (мест {len(mism)}):", file=sys.stderr)
        for b in mism:
            print(b, file=sys.stderr)
        print("  На Linux `sh` — это dash: pipefail, <(...) и [[ в нём — синтаксическая", file=sys.stderr)
        print("  ошибка, код 2, и красный приходит не оттуда, где ошибка (ночной full", file=sys.stderr)
        print("  красен с 2026-10-01, реестр 221.1 №1825). Зови тем, что в шебанге: `bash`.", file=sys.stderr)
    if bad or mism:
        return 1

    print(f"{NAME} ok: самотестов {len(files)}, прямых вызовов стража: 0; "
          f"вызовов `sh` с узнанной целью {calls} в {len(scope)} скриптах, "
          f"целей с шебангом bash среди них: 0")
    return 0


def is_bash(p):
    with open(p, "rb") as fh:
        return b"bash" in fh.readline()


if __name__ == "__main__":
    sys.exit(main())
