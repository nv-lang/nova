#!/usr/bin/env bash
# hardcode-audit.sh — детектор хардкода 7 категорий в компиляторе Nova.
#
# Задача: найти места, где имена типов/протоколов/методов/эффектов продублированы
# из .nv-деклараций прямо в Rust/C — таблицы строк, сравнения, диспетч. Закреплено
# в Plan 196 «Одна правда» как часть долга §3 (есть .nv-декларация + Rust-копия
# = долг; легитим = примитивы/ABI/syslibs, чего в .nv нет).
#
# КЛАССИФИКАЦИЯ 7 КАТЕГОРИЙ (План 196 §554, уточнено волной p196-hardcode-detector):
#
# A. const X: &[&str] — явные списки имён типов/протоколов в виде массивов строк
#    Поиск: grep 'const [A-Z_]*: *&\s*\[.*&str' — эти константы это именно списки.
#
# B. == "ИмяТипа" — сравнения имён типов с литеральными строками
#    Поиск: grep '== *"[A-Z][A-Za-z0-9_]*"' — может быть много ложных срабатываний
#    (типы Result/Option/Self легитимны, но тоже считаются).
#
# C. .contains(&"Имя") — проверка принадлежности имени к набору
#    Поиск: grep 'contains *( *& *"[A-Z]' — похоже на B, но через contains().
#
# D. Рукописный C-vtable/схемы в nova_rt (NovaVtable_*, явно написанные struct)
#    Поиск: grep 'NovaVtable_' в emit_c.rs — конкретные имена таблиц.
#
# E. match name { "Foo" => ... } — имя как ключ в диспетчерском match'е
#    Поиск: grep 'match.*name.*{' с литеральными строками.
#
# F. RUNTIME_DEFINED_TYPES — явное упоминание схем типов в Rust коде
#    Поиск: grep 'RUNTIME_DEFINED_TYPES' — конкретное имя константы/функции.
#
# G. must match / layout-хардкод (ABI, legit for FFI, но риск рассинхрона)
#    Поиск: grep 'must match.*layout' или явные layout-комментарии.
#
# ⚠ УТОЧНЕНИЕ (волна p196-hardcode-detector, 2026-07-22):
# План 196 дал справочные числа ~31/92/21/8/28/41/315 из ручного греп'а
# 2026-07-22. Фактические числа в текущем коде отличаются:
#   — паттерны grep ловят ложные срабатывания (типы в match'ах — Result/Option)
#   — код мог измениться между датой baseline и настоящим моментом
#   — нет точного git-commit'а для baseline
#
# Скрипт — TRIPWIRE, не абсолютный счётчик долга. Главное: если число ВЫРОСЛО,
# это сигнал, что хардкод начал расползаться. Долга/легитима классификация —
# вручную на каждой волне.
#
# ОБНОВЛЕНИЕ BASELINE:
# Если новая волна намеренно добавила хардкод или рефакторинг изменил числа,
# обнови константы BASELINE_* на новые фактические, отметив дату.
#
# ИСПОЛЬЗОВАНИЕ:
# $ bash scripts/hardcode-audit.sh
#   → вывод таблицы 7 категорий, exit 0 если не выросло, 1 если выросло
#
# $ bash scripts/hardcode-audit.sh --list A
#   → показать конкретные сайты (первые 20) категории A
#
# Требования: POSIX bash + grep (никаких внешних зависимостей).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# =====================================================================
# BASELINE (волна p196-hardcode-detector, 2026-07-22)
# =====================================================================
# Обновлено на фактические прогонные значения после греп-обхода репо.
# Эти числа — tripwire-ориентиры, не абсолютные долги.
#
BASELINE_A=39   # const X: &[&str]
BASELINE_B=310  # == "ИмяТипа"  (ВНИМАНИЕ: широкий паттерн, много типов Result/Option)
BASELINE_C=5    # .contains(&"Имя")
BASELINE_D=53   # NovaVtable_* в emit_c.rs (ядро: Time и др.)
BASELINE_E=21   # match name { "Foo"=> }
BASELINE_F=24   # RUNTIME_DEFINED_TYPES (найдено в кодовой базе)
BASELINE_G=0    # must match / layout (слишком общий паттерн, требует уточнения)

# =====================================================================
# ФУНКЦИИ ПОИСКА (по категориям)
# =====================================================================

count_cat_a() {
  # A: const X: &[&str] — явные списки имён
  grep -rn 'const [A-Z_]*: *&\s*\[.*&str' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | wc -l
}

count_cat_b() {
  # B: == "ИмяТипа" — сравнение имён
  # Паттерн включает любые типы, начинающиеся с заглавной буквы (Result, Option и т.д.)
  grep -rn '== *"[A-Z][A-Za-z0-9_]*"' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | wc -l
}

count_cat_c() {
  # C: .contains(&"Имя") — проверка принадлежности
  grep -rn 'contains *( *& *"[A-Z][A-Za-z0-9_]*"' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | wc -l
}

count_cat_d() {
  # D: NovaVtable_* — конкретные vtable'ы в emit_c.rs
  grep -rn 'NovaVtable_' "$REPO_ROOT/compiler-codegen/src/codegen/emit_c.rs" 2>/dev/null | wc -l
}

count_cat_e() {
  # E: match name { "Foo" => ... }
  # Ищем match-выражения с буквенным ключом (упрощённый паттерн)
  grep -rn 'match [a-z_]*name[a-z_]* *{' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | wc -l
}

count_cat_f() {
  # F: RUNTIME_DEFINED_TYPES — конкретное имя констант/функций
  grep -rn 'RUNTIME_DEFINED_TYPES' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | wc -l
}

count_cat_g() {
  # G: must match / layout — очень общий паттерн, требует уточнения
  # Для упрощения пропускаем (слишком широкий паттерн, 315 ABI-легитимных согласно плану)
  echo 0
}

# =====================================================================
# ФУНКЦИИ ВЫВОДА (--list)
# =====================================================================

list_cat_a() {
  grep -rn 'const [A-Z_]*: *&\s*\[.*&str' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | head -20
}

list_cat_b() {
  grep -rn '== *"[A-Z][A-Za-z0-9_]*"' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | head -20
}

list_cat_c() {
  grep -rn 'contains *( *& *"[A-Z][A-Za-z0-9_]*"' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | head -20
}

list_cat_d() {
  grep -rn 'NovaVtable_' "$REPO_ROOT/compiler-codegen/src/codegen/emit_c.rs" 2>/dev/null | head -20
}

list_cat_e() {
  grep -rn 'match [a-z_]*name[a-z_]* *{' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | head -20
}

list_cat_f() {
  grep -rn 'RUNTIME_DEFINED_TYPES' "$REPO_ROOT/compiler-codegen/src" 2>/dev/null | head -20
}

list_cat_g() {
  echo "(категория G не реализована — слишком широкий паттерн)"
}

# =====================================================================
# ОСНОВНОЙ СКРИПТ
# =====================================================================

if [ ! -d "$REPO_ROOT/compiler-codegen/src" ]; then
  echo "hardcode-audit.sh: не найдена директория $REPO_ROOT/compiler-codegen/src" >&2
  exit 1
fi

# Режим --list: вывести конкретные сайты
if [ $# -ge 1 ] && [ "$1" = "--list" ]; then
  if [ $# -lt 2 ]; then
    echo "hardcode-audit.sh: --list требует аргумента (A|B|C|D|E|F|G)" >&2
    exit 1
  fi
  case "$2" in
    A) list_cat_a ;;
    B) list_cat_b ;;
    C) list_cat_c ;;
    D) list_cat_d ;;
    E) list_cat_e ;;
    F) list_cat_f ;;
    G) list_cat_g ;;
    *)
      echo "hardcode-audit.sh: неизвестная категория '$2' (ожидается A|B|C|D|E|F|G)" >&2
      exit 1
      ;;
  esac
  exit 0
fi

# Обычный режим: вывести таблицу и оценить дельту

COUNT_A=$(count_cat_a)
COUNT_B=$(count_cat_b)
COUNT_C=$(count_cat_c)
COUNT_D=$(count_cat_d)
COUNT_E=$(count_cat_e)
COUNT_F=$(count_cat_f)
COUNT_G=$(count_cat_g)

DELTA_A=$((COUNT_A - BASELINE_A))
DELTA_B=$((COUNT_B - BASELINE_B))
DELTA_C=$((COUNT_C - BASELINE_C))
DELTA_D=$((COUNT_D - BASELINE_D))
DELTA_E=$((COUNT_E - BASELINE_E))
DELTA_F=$((COUNT_F - BASELINE_F))
DELTA_G=$((COUNT_G - BASELINE_G))

# Форматирование дельты
format_delta() {
  local d=$1
  if [ "$d" -gt 0 ]; then
    printf '+%d' "$d"
  elif [ "$d" -lt 0 ]; then
    printf '%d' "$d"
  else
    printf '±0'
  fi
}

DELTA_A_STR=$(format_delta "$DELTA_A")
DELTA_B_STR=$(format_delta "$DELTA_B")
DELTA_C_STR=$(format_delta "$DELTA_C")
DELTA_D_STR=$(format_delta "$DELTA_D")
DELTA_E_STR=$(format_delta "$DELTA_E")
DELTA_F_STR=$(format_delta "$DELTA_F")
DELTA_G_STR=$(format_delta "$DELTA_G")

# Таблица вывода
echo "==============================================================="
echo "Аудит хардкода Nova — 7 категорий (План 196 §554)"
echo "Baseline: волна p196-hardcode-detector (2026-07-22)"
echo "==============================================================="
echo
printf "| Кат | Найдено | Baseline | Дельта  | Описание\n"
printf "|-----|---------|----------|---------|--------------------------------------\n"
printf "| A   | %7d | %8d | %7s | const X: &[&str] (списки имён)\n" "$COUNT_A" "$BASELINE_A" "$DELTA_A_STR"
printf "| B   | %7d | %8d | %7s | == \"ИмяТипа\" (сравнения)\n" "$COUNT_B" "$BASELINE_B" "$DELTA_B_STR"
printf "| C   | %7d | %8d | %7s | .contains(&\"Имя\")\n" "$COUNT_C" "$BASELINE_C" "$DELTA_C_STR"
printf "| D   | %7d | %8d | %7s | NovaVtable_* в emit_c\n" "$COUNT_D" "$BASELINE_D" "$DELTA_D_STR"
printf "| E   | %7d | %8d | %7s | match name { \"Foo\"=> }\n" "$COUNT_E" "$BASELINE_E" "$DELTA_E_STR"
printf "| F   | %7d | %8d | %7s | RUNTIME_DEFINED_TYPES\n" "$COUNT_F" "$BASELINE_F" "$DELTA_F_STR"
printf "| G   | %7d | %8d | %7s | must match / layout (ABI)\n" "$COUNT_G" "$BASELINE_G" "$DELTA_G_STR"
printf "\n"

TOTAL=$((COUNT_A + COUNT_B + COUNT_C + COUNT_D + COUNT_E + COUNT_F + COUNT_G))
BASELINE_TOTAL=$((BASELINE_A + BASELINE_B + BASELINE_C + BASELINE_D + BASELINE_E + BASELINE_F + BASELINE_G))
TOTAL_DELTA=$((TOTAL - BASELINE_TOTAL))

printf "| ∑   | %7d | %8d | %7s | Всего хардкод-сайтов\n" "$TOTAL" "$BASELINE_TOTAL" "$(format_delta "$TOTAL_DELTA")"
printf "\n"

echo "КРИТЕРИЙ ДОЛГА (План 196 §3):"
echo "  — ДОЛГ: Rust-копия имени + .nv-декларация типа/протокола/метода"
echo "  — ЛЕГИТИМ: примитивы, ABI-layout, C-syslibs (нет в .nv)"
echo
echo "TRIPWIRE-ГЕЙТ:"

EXIT_CODE=0

if [ "$DELTA_A" -gt 0 ]; then
  echo "  ⚠ Кат.A: хардкод вырос на $DELTA_A (было $BASELINE_A, стало $COUNT_A)"
  EXIT_CODE=1
fi
if [ "$DELTA_B" -gt 0 ]; then
  echo "  ⚠ Кат.B: хардкод вырос на $DELTA_B (было $BASELINE_B, стало $COUNT_B)"
  EXIT_CODE=1
fi
if [ "$DELTA_C" -gt 0 ]; then
  echo "  ⚠ Кат.C: хардкод вырос на $DELTA_C (было $BASELINE_C, стало $COUNT_C)"
  EXIT_CODE=1
fi
if [ "$DELTA_D" -gt 0 ]; then
  echo "  ⚠ Кат.D: хардкод вырос на $DELTA_D (было $BASELINE_D, стало $COUNT_D)"
  EXIT_CODE=1
fi
if [ "$DELTA_E" -gt 0 ]; then
  echo "  ⚠ Кат.E: хардкод вырос на $DELTA_E (было $BASELINE_E, стало $COUNT_E)"
  EXIT_CODE=1
fi
if [ "$DELTA_F" -gt 0 ]; then
  echo "  ⚠ Кат.F: хардкод вырос на $DELTA_F (было $BASELINE_F, стало $COUNT_F)"
  EXIT_CODE=1
fi
if [ "$DELTA_G" -gt 0 ]; then
  echo "  ⚠ Кат.G: хардкод вырос на $DELTA_G (было $BASELINE_G, стало $COUNT_G)"
  EXIT_CODE=1
fi

if [ "$EXIT_CODE" -eq 0 ]; then
  echo "  ✓ Tripwire: OK (хардкод стабилен или уменьшился)"
else
  echo
  echo "  СТОП: хардкод вырос выше baseline. Проверь причину перед push."
fi

exit "$EXIT_CODE"
