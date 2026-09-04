#!/usr/bin/env bash
# scripts/guards/check-cli-output-language.sh — язык ПОСТАВЛЯЕМОГО вывода CLI.
#
# ЗАЧЕМ (реестр 221.1 №823). Правило репозитория (`AGENTS.md` §Language)
# требует английского от диагностических текстов, и механизм на него был —
# но только для СООБЩЕНИЙ КОММИТОВ (`check-commit-language.sh`) и для `.nv`.
# Вывод самого бинаря не судился ничем, и он разошёлся с правилом настолько,
# что этого никто не заметил: на 2026-09-04 в `nova --help` и в помощи её
# девятнадцати подкоманд — 152 строки кириллицы, а в JSON-схеме, которую
# печатает `nova doc --json-schema` и которая несёт внешний
# `"$id": "https://nova-lang.org/schemas/nova-doc-v1.json"`, — ещё 11.
#
# Строка реестра называла «12 строк кириллицы в --help». Это верхний уровень;
# полный замер даёт в тринадцать раз больше. Число в строке реестра — гипотеза
# автора строки, и пересчитывать его надо ДО работы, а не после.
#
# ПОЧЕМУ ПО ВЫВОДУ, А НЕ ПО ИСХОДНИКУ — для help и схемы. По исходнику этот
# вопрос надёжно не решается: `///` внутри `#[derive(Parser)]` печатается
# пользователю, а точно такой же `///` двадцатью строками ниже — обычный
# rustdoc, которого не видит никто. Отличать их приходилось бы по границам
# структур, то есть страж зависел бы от РАСКЛАДКИ файла и врал бы при первой
# же перестановке. Вывод не оставляет места для толкования: что напечаталось,
# то и поставляется.
#
# ПОЧЕМУ ПО ИСХОДНИКУ — для текстов ошибок. Их запуском не перечислить: чтобы
# увидеть каждое сообщение об ошибке, надо вызвать каждую ошибку. Считаются
# строки `nova-cli/src/**/*.rs`, где кириллица стоит В КОДЕ, а не в комментарии
# — то есть литералы в `bail!`/`anyhow!`/`usage_err!`/`eprintln!`. Комментарий
# отбрасывается разбором, а не шаблоном `^//`: подробности и цена ошибки — в
# самом счётчике ниже.
# Две половины не пересекаются: help и схема живут в `///` (это комментарий,
# счётчик исходника его не видит) и попадают в первый счётчик через вывод.
#
# ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Приёмка строки №823 прямо просит «базу-храповик
# на остаток»: разом перевести 163 строки помощи нельзя без вычитки каждой,
# а правка без вычитки — это как раз тот способ получить английский текст,
# который врёт. Число обязано ходить только ВНИЗ; опускать его — часть работы
# того, кто переводит, а не уборка потом.
#
# ЧТО СЧИТАЕТСЯ КИРИЛЛИЦЕЙ: байтовая пара UTF-8 `[\xd0-\xd1][\x80-\xbf]`,
# то есть U+0400..U+04FF. Байтовая проверка выбрана намеренно — `grep -P` с
# классом по кодовым точкам под msys2 зависит от локали, а `LC_ALL=C` плюс
# байты дают один и тот же ответ на любой машине.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-cli-output-language.sh [КОРЕНЬ]
#   bash scripts/guards/check-cli-output-language.sh --selftest

set -u
export LC_ALL=C

SELFTEST=0
ROOT="."
for a in "$@"; do
  case "$a" in
    --selftest) SELFTEST=1 ;;
    *) ROOT="$a" ;;
  esac
done

CYR='[\xd0-\xd1][\x80-\xbf]'
BASELINE="$ROOT/scripts/guards/cli-output-language.baseline"

# Считает строки с кириллицей в поданном на stdin тексте.
count_cyr() { grep -cP "$CYR" || true; }

find_nova() {
  local c
  for c in "$ROOT/nova-cli/target/release/nova.exe" \
           "$ROOT/nova-cli/target/release/nova" \
           "$ROOT/nova-cli/target/debug/nova.exe" \
           "$ROOT/nova-cli/target/debug/nova"; do
    if [ -x "$c" ]; then printf '%s' "$c"; return 0; fi
  done
  return 1
}

read_baseline() {
  local key="$1" v
  v=$(grep -E "^${key}=[0-9]+$" "$BASELINE" 2>/dev/null | tail -1 | cut -d= -f2)
  if [ -z "${v:-}" ]; then echo "MISSING"; else echo "$v"; fi
}

# --- самопроверка: доказывает, что страж УМЕЕТ краснеть -----------------------
if [ "$SELFTEST" = "1" ]; then
  fail=0
  synthetic=$(printf 'plain ascii line\nстрока с кириллицей\nanother ascii\nещё одна\n')
  got=$(printf '%s\n' "$synthetic" | count_cyr)
  if [ "$got" != "2" ]; then
    echo "selftest FAIL: счётчик дал $got вместо 2 на синтетическом тексте"
    fail=1
  else
    echo "selftest ok: счётчик видит кириллицу (2 из 4 строк)"
  fi
  # арифметика храповика — обе стороны
  if [ 5 -gt 4 ]; then echo "selftest ok: рост 5 > 4 распознан как рост"; else
    echo "selftest FAIL: рост не распознан"; fail=1; fi
  if [ 3 -gt 4 ]; then echo "selftest FAIL: падение принято за рост"; fail=1; else
    echo "selftest ok: падение 3 < 4 ростом не считается"; fi
  if [ -f "$BASELINE" ]; then
    echo "selftest ok: база на месте — $BASELINE"
  else
    echo "selftest FAIL: базы нет — $BASELINE"
    fail=1
  fi
  if [ "$fail" = "0" ]; then echo "check-cli-output-language --selftest ok"; exit 0; fi
  echo "check-cli-output-language --selftest FAIL"; exit 1
fi

# --- половина первая: ВЫВОД (help всех подкоманд + JSON-схема) ---------------
NOVA=$(find_nova) || {
  echo "check-cli-output-language: бинарь nova не найден под $ROOT/nova-cli/target/"
  echo "    Страж судит ПОСТАВЛЯЕМЫЙ вывод, значит бинарь обязан быть собран."
  echo "    Собрать: cargo build --release --manifest-path nova-cli/Cargo.toml"
  exit 1
}

out_total=$("$NOVA" --help 2>&1 | count_cyr)
subs=$("$NOVA" --help 2>&1 | sed -n '/^Commands:/,/^Options:/p' \
       | awk '{print $1}' | grep -E '^[a-z][a-z-]+$' || true)
for s in $subs; do
  n=$("$NOVA" "$s" --help 2>&1 | count_cyr)
  out_total=$((out_total + n))
done
schema=$("$NOVA" doc --json-schema 2>&1 | count_cyr)
out_total=$((out_total + schema))

# --- половина вторая: ИСХОДНИК (тексты ошибок CLI) ---------------------------
#
# ПОЧЕМУ ЗДЕСЬ PYTHON, А НЕ GREP. Первая версия отбрасывала комментарии
# условием `^[[:space:]]*//` и на этом промахнулась: ХВОСТОВОЙ комментарий
# (`let x = 1; // пояснение`) она считала кодом. Девять таких строк остались бы
# вечным полом храповика, а хуже того — любой НОВЫЙ русский комментарий в конце
# строки красил бы гейт, хотя правило языка комментариев не касается вовсе.
# Страж, который краснеет на законном, отключают первым же окном.
#
# Отличить хвостовой комментарий от `//` внутри строкового литерала (`"https://"`
# — самый частый случай) нельзя без разбора кавычек, а этого grep не умеет.
# Разбор ниже маленький и точный: посимвольный проход, состояние «внутри строки»,
# экранирование `\"`. Python в стражах уже есть (registry-routes-scan.py,
# check-gate-budget.py), новой зависимости не заводится.
src_total=$(python - "$ROOT/nova-cli/src" <<'PYEOF' || echo BAD
import io, os, re, sys

CYR = re.compile(r"[\u0400-\u04FF]")

def code_part(line):
    """Строка без хвостового `//`-комментария; `//` внутри литерала сохраняется."""
    out = []
    in_str = False
    esc = False
    i = 0
    n = len(line)
    while i < n:
        c = line[i]
        if in_str:
            out.append(c)
            if esc:
                esc = False
            elif c == "\\":
                esc = True
            elif c == '"':
                in_str = False
            i += 1
            continue
        if c == '"':
            in_str = True
            out.append(c)
            i += 1
            continue
        if c == "/" and i + 1 < n and line[i + 1] == "/":
            break
        out.append(c)
        i += 1
    return "".join(out)

root = sys.argv[1]
total = 0
for dirpath, _dirs, files in os.walk(root):
    for f in files:
        if not f.endswith(".rs"):
            continue
        p = os.path.join(dirpath, f)
        for line in io.open(p, encoding="utf-8", errors="replace"):
            if CYR.search(code_part(line)):
                total += 1
print(total)
PYEOF
)
if [ "$src_total" = "BAD" ] || [ -z "$src_total" ]; then
  echo "check-cli-output-language: счётчик исходника не отработал (python?)"
  exit 1
fi

base_out=$(read_baseline cli_output)
base_src=$(read_baseline cli_error_texts)

if [ "$base_out" = "MISSING" ] || [ "$base_src" = "MISSING" ]; then
  echo "check-cli-output-language: база не читается — $BASELINE"
  echo "    Нужны строки вида cli_output=N и cli_error_texts=N."
  exit 1
fi

echo "check-cli-output-language: вывод (--help всех подкоманд + JSON-схема) = $out_total (база $base_out)"
echo "check-cli-output-language: тексты ошибок в nova-cli/src           = $src_total (база $base_src)"

rc=0
if [ "$out_total" -gt "$base_out" ]; then
  echo "check-cli-output-language: РОСТ кириллицы в поставляемом выводе: $base_out -> $out_total"
  echo "    Вывод бинаря читают СНАРУЖИ: репозиторий публичен и зеркалится на три хоста,"
  echo "    а JSON-схема несёт внешний \$id. Новый русский текст в --help или в схеме"
  echo "    не добавляется — пиши по-английски (AGENTS.md, раздел Language)."
  rc=1
fi
if [ "$src_total" -gt "$base_src" ]; then
  echo "check-cli-output-language: РОСТ кириллицы в текстах ошибок CLI: $base_src -> $src_total"
  echo "    Диагностический текст — по-английски (AGENTS.md, раздел Language)."
  rc=1
fi
if [ "$out_total" -lt "$base_out" ] || [ "$src_total" -lt "$base_src" ]; then
  echo "check-cli-output-language: число УПАЛО — опусти базу тем же слиянием,"
  echo "    строкой-летописью, как заведено в самом файле базы. Храповик, оставленный"
  echo "    высоким, молча разрешает откатиться ровно на столько, на сколько ты продвинулся."
fi

if [ "$rc" = "0" ]; then echo "check-cli-output-language ok: роста нет"; fi
exit "$rc"
