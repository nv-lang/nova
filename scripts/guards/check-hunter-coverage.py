#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# scripts/guards/check-hunter-coverage.py — план 278 Ф.3, §15 исследования.
#
# ЗАЧЕМ. Сетка клеток (модуль novac x класс К1..К7) — КАРТА ПОКРЫТИЯ охоты, не
# очередь: клетки, где никогда не охотились, буквально и есть «цели, о которых
# никто не думает», и сегодня они невидимы ровно потому, что не названы.
#
# ВЫВОДИТСЯ ИЗ ДЕРЕВА, НЕ ПИШЕТСЯ РУКАМИ (прецедент
# check-no-handwritten-plan-index.sh): ось модулей — таблица рёбер §3
# docs/dev/novac-architecture.md; ось классов — К1..К7; факт охоты — строка
# «КЛЕТКА | <модуль> | К<n>» в отчёте docs/dev/hunts/novac/*.md (формат
# задаётся здесь и брифом Ф.4 ВМЕСТЕ — бриф ссылается сюда, второго дома нет).
#
# ТОЛЬКО ТРЕК novac (Ф.6, решение владельца 2026-08-30): у оракула нет
# архитектурного документа с таблицей рёбер — оси взять неоткуда, его охоты
# судит только check-hunter-mark.sh. То же у трека guards (Ф.8, 2026-09-04):
# ось «стражи» — список файлов каталога, любая сетка по нему была бы копией
# `ls`. Строк «КЛЕТКА |» в отчёте может быть
# НЕСКОЛЬКО (многоклеточная охота — findall, дыра критика №2 панели);
# свёрнутые охоты продолжают держать клетки строками СВЁРНУТО в LEDGER.md
# (иначе первая же свёртка подняла бы never_hunted над базой).
#
# ЧТО КРАСНИТ:
#   1. отчёт охоты без разбираемой строки «КЛЕТКА |» — по уроку №801 генератор
#      ОТКАЗЫВАЕТ на непонятой форме, а не молчит;
#   2. клетка с модулем, которого нет в таблице рёбер, — охота мимо карты;
#   3. рукописный файл сетки в дереве (второй дом);
#   4. рост числа неохваченных клеток над базой (храповик вниз; база засеяна
#      СЕГОДНЯШНИМ числом, чтобы страж не краснел на всей истории).
#      Рост законен только с правкой базы и хроникой в ней — как у всех баз.
#
# Самотест: selftest/test-check-hunter-coverage.sh.
import io
import os
import re
import subprocess
import sys

# Вывод стабилен в UTF-8 независимо от кодовой страницы консоли (cp1251 под
# Windows молча портит кириллицу в перенаправленном выводе — самотест грепает).
# newline="\n" обязателен, а не украшение: python на Windows пишет CRLF там, где
# shell писал LF, и вывод стража расходится сам с собой между платформами —
# это ловит check-guard-honesty.py и ловил на этом файле 2026-08-30.
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", newline="\n")
    sys.stderr.reconfigure(encoding="utf-8", newline="\n")

ROOT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "..")
ROOT = os.path.abspath(ROOT)
ARCH = os.path.join(ROOT, "docs", "dev", "novac-architecture.md")
HUNTS = os.environ.get("NOVA_HUNTS_DIR", os.path.join(ROOT, "docs", "dev", "hunts", "novac"))
BASE_FILE = os.environ.get("NOVA_HUNTER_GRID_BASELINE",
                           os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                        "hunter-grid.baseline"))
CLASSES = [u"К%d" % i for i in range(1, 8)]

def fail(msg):
    sys.stderr.write(u"check-hunter-coverage: FAIL — %s\n" % msg)
    sys.exit(1)

# ── ось модулей: таблица рёбер §3 ────────────────────────────────────────
try:
    arch = io.open(ARCH, encoding="utf-8", errors="replace").read()
except IOError:
    fail(u"нет %s — ось модулей взять неоткуда" % ARCH)
parts = arch.split(u"## 3.")
if len(parts) < 2:
    # Собственное сообщение вместо трейсбека: страж, объявивший «отказ на
    # непонятой форме», обязан ОБЪЯСНИТЬ отказ, а не показать стек (панель
    # 2026-08-30 воспроизвела IndexError запуском).
    fail(u"в %s нет заголовка «## 3.» — ось модулей взять неоткуда: форма документа уехала, чинить разбор" % ARCH)
sec = parts[1].split(u"\n## ")[0]
modules = sorted(set(re.findall(r"^\|\s*`([a-z_]+)`", sec, re.M)))
if len(modules) < 5:
    fail(u"в §3 таблицы рёбер разобрано только %d модулей — форма таблицы уехала, чинить разбор, а не молчать" % len(modules))

grid_size = len(modules) * len(CLASSES)

# ── факт охоты: отчёты ───────────────────────────────────────────────────
hunted = set()
if os.path.isdir(HUNTS):
    for fn in sorted(os.listdir(HUNTS)):
        if not fn.endswith(".md"):
            continue
        p = os.path.join(HUNTS, fn)
        t = io.open(p, encoding="utf-8", errors="replace").read()
        if fn == "LEDGER.md":
            # свёрнутые охоты: клетка — поля 3–4 строки СВЁРНУТО (формат — дом
            # check-hunter-fold.sh); пустой леджер законен.
            cells = re.findall(u"^СВЁРНУТО \\|[^|]*\\| *([a-z_]+) *\\| *(К[1-7])\\b", t, re.M)
        else:
            cells = re.findall(u"^КЛЕТКА \\| *([a-z_]+) *\\| *(К[1-7])\\b", t, re.M)
            if not cells:
                fail(u"отчёт %s без разбираемой строки «КЛЕТКА | <модуль> | К<n>» — отказ на непонятой форме (№801)" % fn)
        for mod, cls in cells:
            if mod not in modules:
                if fn == "LEDGER.md":
                    # Леджер — запись о ПРОШЛОМ, а ось модулей живёт: честное
                    # переименование модуля в §3 краснило стража навсегда на
                    # замороженной истории (панель 2026-08-30, проверено
                    # запуском). Клетку не засчитываем — храповик поймает рост
                    # неохваченного, если ось действительно уехала.
                    sys.stderr.write(
                        u"check-hunter-coverage: замечание — свёрнутая охота по модулю «%s», "
                        u"которого больше нет в таблице рёбер §3: клетка не засчитана\n" % mod)
                    continue
                fail(u"отчёт %s называет модуль «%s», которого нет в таблице рёбер §3 (%s)" % (fn, mod, u", ".join(modules)))
            hunted.add((mod, cls))

never = grid_size - len(hunted)

# ── рукописная сетка — второй дом ────────────────────────────────────────
try:
    ls = subprocess.run(["git", "-C", ROOT, "ls-files"], capture_output=True, text=True).stdout
    hand = [l for l in ls.split("\n")
            if re.search(r"hunter.?grid", l, re.I) and l.endswith(".md")]
    if hand:
        fail(u"рукописная сетка в дереве: %s — сетка только выводится, вторых домов нет" % hand[0])
except OSError:
    pass  # без git судим остальное

# ── храповик ─────────────────────────────────────────────────────────────
try:
    base_t = io.open(BASE_FILE, encoding="utf-8", errors="replace").read()
    m = re.search(r"^never_hunted=(\d+)", base_t, re.M)
    base = int(m.group(1)) if m else None
except IOError:
    base = None
if base is None:
    fail(u"нет базы %s (ключ never_hunted=N) — храповик судить нечем" % BASE_FILE)

# Размер оси, с которым база засеяна. №816: ось выводится из документа, который
# в РАЗНЫХ ВЕТКАХ разный (на main 14 модулей, на p274-novac 15 — там есть
# `resolve`), а база — один файл на обе. Без этого ключа расхождение читается
# как ошибка ЧИСЛА, и окно 274 потратило на такое расследование время
# 2026-08-30. Ключ необязателен (старые базы его не несут), но если он есть —
# сообщение о росте называет причину, а не заставляет её угадывать.
m_seed = re.search(r"^modules_at_seed=(\d+)", base_t, re.M) if base is not None else None
seed_mods = int(m_seed.group(1)) if m_seed else None

if never > base:
    why = u""
    if seed_mods is not None and len(modules) != seed_mods:
        why = (u" ОСЬ ИЗМЕНИЛАСЬ: в базе modules_at_seed=%d, в дереве %d модулей — "
               u"это правка таблицы рёбер §3 (модуль заведён или снят), а НЕ ошибка "
               u"числа в базе: каждый модуль стоит %d клеток. Проверь, ту ли ветку ты "
               u"судишь: документ ветко-зависим, база одна на все."
               % (seed_mods, len(modules), len(CLASSES)))
    elif seed_mods is None:
        why = (u" В базе нет ключа modules_at_seed=N, поэтому назвать причину — "
               u"выросла ОСЬ или отстало ЧИСЛО — страж не может; допиши ключ той же "
               u"правкой (№816).")
    fail(u"неохваченных клеток стало БОЛЬШЕ: %d > базы %d (модулей %d x классов %d).%s Двигай базу С ХРОНИКОЙ."
         % (never, base, len(modules), len(CLASSES), why))

if seed_mods is not None and len(modules) != seed_mods:
    fail(u"ось модулей в дереве (%d) разошлась с записанной в базе (modules_at_seed=%d), "
         u"а неохваченных при этом не выросло. Молчать нельзя: значит база двигалась под "
         u"ДРУГУЮ ось. Обнови modules_at_seed С ХРОНИКОЙ (№816)."
         % (len(modules), seed_mods))

print(u"check-hunter-coverage ok: модулей %d, клеток %d, охочено %d, никогда не охочено %d (база %d) — карта выведена из дерева"
      % (len(modules), grid_size, len(hunted), never, base))
