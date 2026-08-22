# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-import-exists.py — каждое ИМПОРТИРУЕМОЕ имя
существует (П5-класс; замер 2026-08-22).

ЗАЧЕМ. Перепроверка сделанной работы по указанию владельца вскрыла, что три
списка импорта в `novac/src` продолжали называть `candidates_from` — функцию,
удалённую днём раньше, — и ВЕСЬ ГЕЙТ БЫЛ ЗЕЛЁНЫЙ. Проба на самом novac: если
вписать в импорт `no_such_name_at_all`, `nova check` отвечает `ok`. То есть оракул
не сверяет, существует ли импортируемое имя, а список импорта — это контракт: и
опечатка в нём, и имя, удалённое из экспортирующего модуля, проходят молча.

Оракульская дыра записана в реестр расхождений и эскалирована; здесь — механизм,
который держит правило до её починки, потому что молчащий контракт хуже
отсутствующего: он выглядит проверенным.

ДВА ПРАВИЛА, и второе нашла та же перепроверка.

ПРАВИЛО A (существование). Для каждой строки `import <путь>.{A, B, C}` в
`novac/src/**/*.nv` каждое имя обязано быть объявлено в модуле, на который
указывает путь: `export type`, `export fn` (в т.ч. метод `export fn T @m`),
`export const` — или быть плечом `export type ... enum` (плечи импортируются
наравне с именами: `DefFn`, `TkNewtype`).

ПРАВИЛО B (использование). Имя, которое МОДУЛЬ не использует нигде, в списке не
стоит. Замер 2026-08-22: 36 таких имён, и 26 из них — мои, добавленные в тот же
день «набором»: заворачивая пространство, я вписывал во все файлы сразу
`X`+`raw_X`+`no_X`+`is_X`, хотя половина файлов пользуется одним. Ровно та же
привычка, что за сессию породила три ЭКСПОРТА без вызывающего, только в третьей
форме — поэтому правило и механизм, а не памятка.

ГРАНУЛЯРНОСТЬ — МОДУЛЬ, И ЭТО НЕСУЩЕЕ. Импорты в Nova видны всему модулю: папка
есть один модуль из со-равных файлов, и файл законно пользуется именем, которое
импортировал СОСЕД (замер: `check.nv` использует `NameTable`, импортированный
`typing.nv`; `slots.nv` — `TyId` из `sem.nv`). Первый замер этого стража считал
по ФАЙЛУ и дал 224 «неиспользуемых» — то есть 188 ложных. Считать по файлу здесь
значит покрасить законный код.

СЛЕПЫЕ ЗОНЫ, названные вслух: имена из `std`/прелюдии страж не судит (модуль
`novac/src` их не объявляет); путь, ведущий вне `novac/src`, пропускается.

$1 — корень; $2 — override директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-import-exists"

RE_IMPORT = re.compile(r"^import\s+((?:\.\./|\./)*)([A-Za-z_][A-Za-z0-9_./]*)\.\{([^}]*)\}\s*$")
RE_EXPORT = re.compile(r"^export\s+(?:type|const)\s+([A-Za-z_][A-Za-z0-9_]*)")
RE_EXPORT_FN = re.compile(r"^export\s+fn\s+(?:[A-Za-z_][A-Za-z0-9_\[\]]*\s+(?:mut\s+|consume\s+|ro\s+)?)?@?([A-Za-z_][A-Za-z0-9_]*)")
RE_ARM = re.compile(r"^\s*\|\s*([A-Z][A-Za-z0-9_]*)")
RE_ENUM_OPEN = re.compile(r"^export\s+type\s+[A-Za-z_][A-Za-z0-9_]*\s+enum\s*$")


def module_names(src, mod):
    """Все имена, которые модуль (папка или файл) отдаёт наружу."""
    base = src / mod
    files = []
    if base.is_dir():
        files = sorted(base.glob("*.nv"))
    elif base.with_suffix(".nv").is_file():
        files = [base.with_suffix(".nv")]
    names = set()
    for f in files:
        in_enum = False
        for line in f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n"):
            m = RE_EXPORT.match(line)
            if m:
                names.add(m.group(1))
            m = RE_EXPORT_FN.match(line)
            if m:
                names.add(m.group(1))
            if RE_ENUM_OPEN.match(line):
                in_enum = True
                continue
            if in_enum:
                m = RE_ARM.match(line)
                if m:
                    names.add(m.group(1))
                elif line.strip():
                    in_enum = False
    return names, bool(files)


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME}: FAIL — в {src} нет ни одного .nv: страж потерял мишень (класс №519)",
              file=sys.stderr)
        return 1

    cache = {}
    bad = []
    checked = 0
    # Правило B считается по МОДУЛЮ (папке): имена видны всем со-равным файлам.
    mod_imports = {}        # модуль -> {имя: "файл:строка"}
    mod_code = {}           # модуль -> код модуля без строк импорта
    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        home = f.parent.name if f.parent != src else "main"
        lines = f.read_bytes().decode("utf-8", "replace").replace("\r", "").split("\n")
        mod_code.setdefault(home, [])
        mod_code[home].append("\n".join(x.split("//", 1)[0] for x in lines
                                        if not RE_IMPORT.match(x)))
        mod_imports.setdefault(home, {})
        for n, line in enumerate(lines, 1):
            m = RE_IMPORT.match(line)
            if not m:
                continue
            for raw0 in m.group(3).split(","):
                nm0 = raw0.strip()
                if nm0:
                    mod_imports[home].setdefault(nm0, f"{rel}:{n}")
            mod = m.group(2)
            if mod not in cache:
                cache[mod] = module_names(src, mod)
            names, found = cache[mod]
            if not found:
                continue                      # путь вне novac/src: названная слепая зона
            for raw in m.group(3).split(","):
                nm = raw.strip()
                if not nm:
                    continue
                checked += 1
                if nm not in names:
                    bad.append(f"  {rel}:{n}: `{nm}` импортируется из `{mod}`, "
                               f"а модуль его не объявляет")

    dead = []
    for home, names in sorted(mod_imports.items()):
        code = "\n".join(mod_code.get(home, []))
        for nm, where in sorted(names.items()):
            if not re.search(r"(^|[^A-Za-z0-9_])" + re.escape(nm) + r"($|[^A-Za-z0-9_])", code):
                dead.append(f"  {where}: `{nm}` импортируется, а модуль `{home}` его нигде не использует")

    if dead and not bad:
        print(f"{NAME}: FAIL — имя импортировано и не используется (правило B):", file=sys.stderr)
        for b in dead:
            print(b, file=sys.stderr)
        print("  Считается по МОДУЛЮ: импорт виден всем со-равным файлам папки, поэтому", file=sys.stderr)
        print("  «сосед импортировал, я пользуюсь» — законно. Незаконно другое: имя,", file=sys.stderr)
        print("  которого не знает НИ ОДИН файл модуля. Так набирается набор `X`+`raw_X`+", file=sys.stderr)
        print("  `no_X`+`is_X` там, где нужен один — та же привычка, что делает экспорт", file=sys.stderr)
        print("  без вызывающего.", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — импортируется имя, которого нет:", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Список импорта — КОНТРАКТ, и оракул его не сверяет: проба 2026-08-22", file=sys.stderr)
        print("  показала, что `import ../types.{no_such_name_at_all}` проходит `check`", file=sys.stderr)
        print("  молча. Так три списка неделю называли удалённую `candidates_from`, и", file=sys.stderr)
        print("  гейт был зелёный. Либо имя есть, либо его нет в списке.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, импортируемых имён проверено: {checked}, "
          f"несуществующих: 0, неиспользуемых модулем: 0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
