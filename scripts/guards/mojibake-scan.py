# -*- coding: utf-8 -*-
"""Ядро check-no-mojibake.sh: печатает строки с подписью «UTF-8 как CP1251».

Разбор на питоне, а не грепом, НАМЕРЕННО: образец состоит из кириллических
символов, а `grep` под `LC_ALL=C` работает побайтово и на не-ASCII образцах
ведёт себя непредсказуемо — этот класс уже стоил проекту ложных «ноль находок»
(см. шапку check-no-handwritten-plan-index.sh про классы [а-я] под LC_ALL=C).
Питон читает файл ЯВНЫМ UTF-8 и сравнивает кодовые точки.

Вывод: по строке на находку, формата `путь:номер`. Пусто — находок нет.
"""
import io
import os
import re
import sys

# Буквы сербско-македонского ряда и одиночная нижняя кавычка перед кириллицей:
# в правильном русском тексте не встречаются, а при порче появляются всегда.
SIG = re.compile(u"[ЂЃђѓћќљњїѕ]"
                 u"|‚[Ѐ-ӿ]")

EXTS = (".md", ".sh", ".py", ".awk", ".baseline", ".yml", ".yaml", ".toml", ".nv", ".rs")

# Ремонтный инструмент ХРАНИТ образцы порчи как данные — краснеть на нём
# значит учить обходить стража целиком.
# Файлы, которые ХРАНЯТ образцы порчи как данные. `mojibake.baseline` попал
# сюда не сразу: его летопись перечисляет буквы-подпись, и первый же прогон
# насчитал на два больше — страж нашёл сам себя. Ровно тот случай, когда
# описание проверки неотличимо от нарушения.
SKIP_FILES = ("demojibake.py", "mojibake-scan.py",
              "test-check-no-mojibake.sh", "mojibake.baseline",
              "check-no-mojibake.sh")
SKIP_DIRS = ("target", ".git", "node_modules", "vcpkg_installed", ".claude")


def main():
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    hits = []
    for base, dirs, files in os.walk(root):
        dirs[:] = [d for d in dirs if d not in SKIP_DIRS]
        for f in files:
            if not f.endswith(EXTS) or f in SKIP_FILES:
                continue
            p = os.path.join(base, f)
            try:
                text = io.open(p, encoding="utf-8").read()
            except Exception:
                continue
            for i, line in enumerate(text.split(u"\n"), 1):
                if SIG.search(line):
                    hits.append(u"%s:%d" % (os.path.relpath(p, root).replace("\\", "/"), i))
    out = io.open(sys.stdout.fileno(), "w", encoding="utf-8", newline="\n", closefd=False)
    for h in hits:
        out.write(h + u"\n")
    out.flush()


if __name__ == "__main__":
    main()
