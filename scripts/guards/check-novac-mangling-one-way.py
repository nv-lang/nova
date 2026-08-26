# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-mangling-one-way.py — дверь мэнглинга
ОДНОСТОРОННЯЯ: C-имя пишут, но не разбирают обратно (конвенция П24).

ПРАВИЛО (перенесено из shell-редакции слово в слово, 2026-08-19). Идентичность
живёт в РЕЕСТРЕ, а не в строке. Красным судится всё, чем C-имя пробуют читать:
поиск подстроки `Nova_`/`NovaValue_`, разбор по разделителю мономорфизации
`____`, срез приставки, сравнение с ABI-именем, и строковые операции прямо на
результате двери `c_type`/`c_struct`/`c_method`/`c_fn`/`c_tag`/`c_maker`.
Комментарии не судятся.

ПОЧЕМУ PYTHON: shell-редакция поднимала `tr` и `awk` на КАЖДЫЙ файл — 2.4с на
дереве из 32 файлов, где работы на доли секунды (П14: скорость первична, и
гейт, который её стережёт, был самым медленным в комнате).

$1 — корень репозитория; $2 — override пути к novac/src (шов самотеста).
"""
import pathlib
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-mangling-one-way"

RULES = (
    (re.compile(r'(starts_with|contains|find|index_of)\(\s*"(Nova_|NovaValue_)'),
     "строковая проверка ABI-приставки"),
    # ЧТЕНИЕ по разделителю, не ЗАПИСЬ (сужено 2026-08-26): дверь имени инстанса
    # ОБЯЗАНА писать `____` -- это разделитель оракула (`Nova_Vec____nova_int`,
    # снят с оболочки), и `s.append("____")` есть дверь, делающая свою работу.
    # Прежний образец ловил любой литерал и покраснел на первой же записи.
    (re.compile(r'(split|find|index_of|contains|starts_with|ends_with|rfind)\(\s*"____"'),
     "разбор по разделителю мономорфизации ____"),
    (re.compile(r'(strip|trim_start|trim_prefix)[a-z_]*\(\s*"(Nova_|NovaValue_)'),
     "срезание ABI-приставки"),
    # НЕ судится в `*_test.nv` (2026-08-26): тест, сравнивающий выход двери с
    # написанием, СНЯТЫМ С ОБОЛОЧКИ, -- это спецификация интеропа, а не дверь,
    # читающая имя обратно. Код по-прежнему судится: решение по ABI-строке в
    # коде остаётся тем, что правило запрещает.
    (re.compile(r'==\s*"(Nova_|NovaValue_)'),
     "сравнение с ABI-именем как со значением"),
    (re.compile(r'c_(type|struct|method|fn|tag|maker)\([^)]*\)\.[a-z_]+\('),
     "строковая операция прямо на результате двери мэнглинга"),
)


def main():
    a = sys.argv
    root = pathlib.Path(a[1] if len(a) > 1 else ".").resolve()
    src = pathlib.Path(a[2]) if len(a) > 2 else root / "novac" / "src"

    if not src.is_dir():
        print(f"{NAME} ok: судить нечего (нет {src})")
        return 0

    files = sorted(src.rglob("*.nv"))
    if not files:
        print(f"{NAME} ok: судить нечего (в {src} файлов .nv: 0)")
        return 0

    bad = []
    for p in files:
        rel = p.relative_to(src).as_posix()
        for i, raw in enumerate(p.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n"), 1):
            line = re.sub(r"//.*$", "", raw)      # комментарии не судятся
            if not line.strip():
                continue
            for rx, why in RULES:
                if rx.search(line):
                    # Правило 4 в тесте -- спецификация, не разбор (см. RULES).
                    if why.startswith("сравнение с ABI") and rel.endswith("_test.nv"):
                        continue
                    bad.append(f"  {rel}:{i}: {why}: {line.strip()[:60]}")
                    break

    if bad:
        print(f"{NAME}: FAIL — C-имя разбирается обратно (конвенция П24):", file=sys.stderr)
        for b in bad:
            print(b, file=sys.stderr)
        print("  Идентичность живёт в РЕЕСТРЕ, а не в строке: спрашивай сущность", file=sys.stderr)
        print("  (голову конструктора, строку вызываемого), а не имя. Дверь — это", file=sys.stderr)
        print("  функция ИЗ сущности В строку, и обратного хода у неё нет.", file=sys.stderr)
        print("  Образец: rustc резолвит по Ty/DefId, rustc_symbol_mangling односторонний.", file=sys.stderr)
        return 1

    print(f"{NAME} ok: файлов .nv: {len(files)}, разборов C-имени: 0 (дверь односторонняя)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
