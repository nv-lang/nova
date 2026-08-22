# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-no-global-state.py — фазы novac не делят
изменяемое состояние.

ПРАВИЛО (план 274 §4 п.5; страж назван в §10.3): если фазы правят общий
контекст, переиспользовать нельзя ничего — принимается сразу или не достигается
никогда. Изменяемым состоянием прохода владеет драйвер (main + pipeline); фаза
получает значения и возвращает значения.

⚖ ПРАВИЛО СУДИТСЯ ПРИЁМКОЙ. Целиком грепом оно не проверяемо: «общий изменяемый
контекст» в Nova выглядит как обычная структура, протянутая через сигнатуры фаз,
и отличить контекст от локального аккумулятора может только чтение. Страж
проверяет ЕДИНСТВЕННОЕ машинное следствие правила — см. ПРОВЕРЯЕТ п.1 — и не
притворяется, что закрывает правило.

ПРОВЕРЯЕТ:
 1. (работающая часть) Изменяемый АГРЕГАТ не протянут через две фазы: собирает
    mut-параметры функций вида `fn f(... mut x TypeName ...)`, где TypeName —
    тип, ОБЪЯВЛЕННЫЙ в самом novac/src (список типов — из данных, `type X` /
    `export type X`, не зашит в страже); модулем считается объявление
    `module ...` в файле, main и pipeline склеиваются в один владелец «driver».
    Если один такой тип стоит mut-параметром в двух и более модулях — красный:
    это и есть контекст, который правят разные фазы.
 2. (дешёвая страховка) Подстрока `static mut` и top-level mut-биндинг
    (`mut `/`export mut ` с колонки 0). Таких форм в Nova СЕГОДНЯ НЕТ — это не
    работающая проверка, а капкан на заимствование из Rust при будущем
    расширении языка; числить её работающей нельзя (дефект F11, честная
    формулировка — 2026-08-15). Имя, совпавшее со строкой novac/GLOBALS.allow
    (одно имя на строку; пустые и '#'-строки игнорируются; при override-скане
    файл ищется рядом со сканируемой директорией), — зелёное write-once
    исключение.

НЕ ПРОВЕРЯЕТ: mut внутри fn-тел (локальная изменяемость законна);
mut-параметры-СТОКИ — `[]T` (вектор-аккумулятор) и типы, не объявленные в
novac/src (StringBuilder и прочий std): сток вывода — не контекст фазы;
mut-получателей `fn T mut @m()` — это метод типа на себе, а не протаскивание
состояния; сигнатуры, разорванные на несколько строк (греп судит строку
объявления); тип, честно живущий mut внутри ОДНОГО модуля (это не «между
фазами»); протаскивание mut-агрегата внутри пары main+pipeline (драйверу
состояние прохода держать можно); write-once-ность исключений из GLOBALS.allow
(заявка — на совести приёмки). Нет novac/src или нет .nv-файлов — зелёный
«судить нечего»: страж до кода легален, молчание нелегально (№645).

ПОЧЕМУ PYTHON: awk-проход уже был один, но разбор его вывода поднимал процесс
на КАЖДОЕ совпадение (`grep -qFx` на строку USES) — 1.8с там, где работы на
50мс (П14).

$1 — корень репозитория; $2 — override сканируемой директории (шов самотеста).
"""
import os
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-no-global-state"

RE_MODULE = re.compile(r"^module[ \t]+(\S+)")
RE_MOD_TAIL = re.compile(r"[^A-Za-z0-9_.].*$")
RE_TOPMUT = re.compile(r"^(export )?mut ")
RE_EXPORT_MUT = re.compile(r"^export mut[ \t]+([A-Za-z_][A-Za-z0-9_]*)")
RE_PLAIN_MUT = re.compile(r"^mut[ \t]+([A-Za-z_][A-Za-z0-9_]*)")
RE_STATIC_MUT = re.compile(r"static[ \t]+mut[ \t]+([A-Za-z_][A-Za-z0-9_]*)")
RE_TYPE = re.compile(r"^[ \t]*(?:export[ \t]+)?type[ \t]+([A-Z][A-Za-z0-9_]*)")
RE_FN = re.compile(r"^[ \t]*(?:export[ \t]+)?fn[ \t]")
RE_MUTPARAM = re.compile(r"mut [a-z_][A-Za-z0-9_]* ([A-Z][A-Za-z0-9_]*)")

DRIVER = {"novac", "novac.pipeline", "novac.main", "pipeline", "main", "."}


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"
    allow = src.parent / "GLOBALS.allow"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src}, файлов .nv: 0)")
        return 0

    files = []
    for dirpath, _dirs, names in os.walk(src):
        for nm in names:
            if nm.endswith(".nv"):
                files.append(pathlib.Path(dirpath) / nm)
    files.sort(key=lambda p: str(p).replace("\\", "/"))

    if not files:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    allowed = set()
    if allow.is_file():
        for line in allow.read_text(encoding="utf-8", errors="replace").split("\n"):
            line = line.rstrip("\r")
            if line.strip() and not line.lstrip().startswith("#"):
                allowed.add(line)

    bad = []
    declared = set()
    uses = []                                  # (тип, модуль, где)

    for f in files:
        rel = str(f.relative_to(src)).replace("\\", "/")
        mod = ""
        for n, raw in enumerate(f.read_bytes().decode("utf-8", "replace").split("\n"), 1):
            if raw.endswith("\r"):
                raw = raw[:-1]
            if not mod:
                m = RE_MODULE.match(raw)
                if m:
                    mod = RE_MOD_TAIL.sub("", m.group(1))

            # (а) глобальное изменяемое состояние
            if RE_TOPMUT.match(raw) or "static mut" in raw:
                m = RE_EXPORT_MUT.match(raw) or RE_PLAIN_MUT.match(raw) or RE_STATIC_MUT.search(raw)
                name = m.group(1) if m else ""
                if name == "" or name not in allowed:
                    bad.append(f"  {rel}:{n}: {raw}")

            line = re.sub(r"//.*$", "", raw)

            # (б) типы, объявленные самим novac
            m = RE_TYPE.match(line)
            if m:
                declared.add(m.group(1))

            # (в) mut-параметры в сигнатурах
            if RE_FN.match(line):
                owner = mod if mod else rel.rsplit("/", 1)[0] if "/" in rel else rel
                if not mod:
                    owner = re.sub(r"/[^/]*$", "", rel)
                if owner in DRIVER:
                    owner = "driver"
                for m in RE_MUTPARAM.finditer(line):
                    uses.append((m.group(1), owner, f"{rel}:{n}"))

    if bad:
        print(f"{NAME}: FAIL — общее изменяемое состояние (274 §4 п.5):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Фазы не делят изменяемый контекст: состояние течёт значениями по", file=sys.stderr)
        print("  рёбрам карты. Write-once исключение — имя строкой в novac/GLOBALS.allow.", file=sys.stderr)
        return 1

    uses = [u for u in uses if u[0] in declared]
    nuses = len(uses)
    ntypes = len({u[0] for u in uses})

    mods_of = {}
    for ty, owner, _where in uses:
        mods_of.setdefault(ty, set()).add(owner)
    shared = sorted(ty for ty, ms in mods_of.items() if len(ms) > 1)

    if shared:
        print(f"{NAME}: FAIL — изменяемый агрегат протянут через несколько фаз (274 §4 п.5):", file=sys.stderr)
        for ty in shared:
            mods = " ".join(sorted(mods_of[ty])) + " "
            print(f"  {ty}: mut-параметр в модулях: {mods}", file=sys.stderr)
            for t, m, where in uses:
                if t == ty:
                    print(f"      {where} ({m})", file=sys.stderr)
        print("  Состояние прохода держит драйвер (main+pipeline); фаза берёт значения", file=sys.stderr)
        print("  и возвращает значения. Либо сделай параметр немутируемым, либо оставь", file=sys.stderr)
        print("  агрегат внутри одного модуля.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, глобальных mut вне GLOBALS.allow: 0, "
          f"mut-агрегатов в сигнатурах: {nuses} (типов: {ntypes}), протянутых через две фазы: 0 "
          f"(⚖ остальное судит приёмка)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
