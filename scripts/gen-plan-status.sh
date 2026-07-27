#!/usr/bin/env bash
# gen-plan-status.sh — генерирует docs/plans/STATUS.md из пофайловых
# `**Статус:**`-строк планов. Единственный источник статуса плана — сам
# файл плана (docs/plans/NNN-*.md); этот скрипт только собирает обзор.
#
# Требования: только POSIX-инструменты (bash/grep/sed/sort/cut/wc) + awk
# (стандартный POSIX-утилита, используется только для вычисления
# сортировочного ключа подномеров плана). Никаких внешних зависимостей.
#
# Идемпотентно и детерминированно: тот же docs/plans/*.md → тот же вывод.
#
# Часть семейства машинных стражей плана 231 «Выход из цикла точечных
# фиксов» (docs/plans/231-bug-cycle-exit.md, трек Д «машинное принуждение
# норм»; docs/plans/231.2-enforcement-infra.md — исполнительный дом трека Д).
# Сама норма «статус плана — только пофайлово, сводка только генератором» —
# в docs/conventions-governance.md; этот скрипт и есть тот генератор,
# на который она ссылается, и парный страж — check-no-manual-status-table.sh
# (не даёт вернуться к рукописной сводной таблице).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PLANS_DIR="$REPO_ROOT/docs/plans"
OUT_FILE="$PLANS_DIR/STATUS.md"

if [ ! -d "$PLANS_DIR" ]; then
  echo "gen-plan-status.sh: не найдена директория $PLANS_DIR" >&2
  exit 1
fi

ROWS_TMP="$(mktemp)"
KEYED_TMP="$(mktemp)"
trap 'rm -f "$ROWS_TMP" "$KEYED_TMP"' EXIT

# ---------------------------------------------------------------------
# Безопасная (UTF-8-целостная) обрезка байтовой строки: инструменты этого
# окружения (cut/awk/sed) режут по БАЙТАМ, а не по символам, даже под
# UTF-8-локалью — поэтому после cut -c нужно доубрать «хвост» оборванной
# многобайтовой последовательности.
utf8_safe_trim() {
  sed -E '
    s/[\xF0-\xF7][\x80-\xBF]{0,2}$//;
    s/[\xE0-\xEF][\x80-\xBF]{0,1}$//;
    s/[\xC2-\xDF]$//
  '
}

for f in "$PLANS_DIR"/*.md; do
  [ -e "$f" ] || continue
  base="$(basename "$f")"

  # --- Исключить служебные не-планы ---
  case "$base" in
    README.md|STATUS.md)
      continue
      ;;
    *-notes.md|*-progress.md|*-execution-plan.md|*-session*.md)
      # заметки/чекпоинты/сессионные логи — не планы (см. docs/plans/*-notes.md
      # и подобные суффиксы: *-notes, *-progress, *-execution-plan, *-session*)
      continue
      ;;
  esac

  # --- Номер плана: ведущий числовой id, точки могут разделять как
  # чисто-числовые (100.4.1), так и буквенные под-номера (57.E.2, 62.A.bis,
  # 91.8a) — так называются реальные под-планы в репозитории. ---
  num="$(printf '%s' "$base" | grep -oE '^[0-9][0-9A-Za-z]*(\.[0-9A-Za-z]+)*-' || true)"
  num="${num%-}"
  if [ -z "$num" ]; then
    # имя файла не начинается с номера плана — не план (README/ROADMAP/
    # backlog/служебные d-документы и т.п.)
    continue
  fi

  # --- Название: первый `# ...` заголовок ---
  title_raw="$(grep -m1 -E '^# ' "$f" || true)"
  if [ -z "$title_raw" ]; then
    title="(нет заголовка)"
  else
    title="$(printf '%s' "$title_raw" | sed -E '
      s/^#+[[:space:]]*//;
      s/\*\*//g;
      s/`//g;
      s/\[([^]]*)\]\([^)]*\)/\1/g;
      s/[[:space:]]+$//
    ')"
  fi

  # --- Статус: первая строка с **Статус:** (учитывая `> **Статус:**`) ---
  status_line="$(grep -m1 -E '\*\*Статус:\*\*' "$f" || true)"
  if [ -z "$status_line" ]; then
    status="— (нет Статус-строки)"
  else
    status_full="$(printf '%s' "$status_line" | sed -E '
      s/^>[[:space:]]*//;
      s/.*\*\*Статус:\*\*[[:space:]]*//;
      s/[[:space:]]+$//
    ')"
    status_cut="$(printf '%s' "$status_full" | cut -c1-320 | utf8_safe_trim)"
    orig_bytes="$(printf '%s' "$status_full" | wc -c)"
    cut_bytes="$(printf '%s' "$status_cut" | wc -c)"
    if [ "$orig_bytes" -gt "$cut_bytes" ]; then
      status="${status_cut}…"
    else
      status="$status_cut"
    fi
  fi

  # --- Экранировать `|`, чтобы не ломать markdown-таблицу ---
  title_esc="$(printf '%s' "$title" | sed 's/|/\\|/g')"
  status_esc="$(printf '%s' "$status" | sed 's/|/\\|/g')"

  printf '%s\t%s\t%s\t%s\n' "$num" "$base" "$title_esc" "$status_esc" >> "$ROWS_TMP"
done

# ---------------------------------------------------------------------
# Натуральная числовая сортировка по номеру плана (172 < 172.1 < 172.2 <
# 172.12 < 173), учитывая до 5 уровней под-номеров. Точки внутри
# Названия/Статуса не должны участвовать в разбиении на поля — поэтому
# сортировочный ключ считается ОТДЕЛЬНО (по колонке 1, TSV) и
# приклеивается первой колонкой перед sort, затем срезается.
#
# Буквенные под-номера (57.E.2, 62.A.bis, 91.8a) сортируются по своей
# числовой части (буквы игнорируются для ключа) — приблизительно, но
# стабильно; такие планы — редкое исключение (см. отчёт).
awk -F'\t' '
{
  n = split($1, segs, ".")
  key = ""
  for (i = 1; i <= 5; i++) {
    seg = (i <= n) ? segs[i] : ""
    d = "0"
    if (match(seg, /^[0-9]+/)) {
      d = substr(seg, RSTART, RLENGTH) + 0
    }
    key = key sprintf("%08d.", d)
  }
  print key "\t" $0
}
' "$ROWS_TMP" | sort -t "$(printf '\t')" -k1,1 | cut -f2- > "$KEYED_TMP"

{
  echo '<!-- AUTO-GENERATED — НЕ РЕДАКТИРОВАТЬ РУКАМИ. Регенерация: bash scripts/gen-plan-status.sh -->'
  echo
  echo '# Статусы планов (сводный обзор)'
  echo
  echo "> **Автосгенерировано**: \`bash scripts/gen-plan-status.sh\`, дата генерации: $(date -u '+%Y-%m-%d %H:%M UTC')."
  echo '> **⚠ Этот файл ПРОТУХАЕТ между перегенерациями** — git-копия отражает момент'
  echo '> последнего запуска, не текущее состояние. Источник правды — ТОЛЬКО строка'
  echo '> `**Статус:**` в самом файле плана; при любом сомнении — перегенерируй или'
  echo '> читай план напрямую. Редактировать руками бессмысленно — следующий запуск'
  echo '> перезапишет.'
  echo
  echo '| План | Название | Статус |'
  echo '|---|---|---|'
  while IFS="$(printf '\t')" read -r num base title status; do
    printf '| [%s](%s) | %s | %s |\n' "$num" "$base" "$title" "$status"
  done < "$KEYED_TMP"
} > "$OUT_FILE"

count="$(wc -l < "$KEYED_TMP" | tr -d '[:space:]')"
echo "gen-plan-status.sh: записано $count планов в $OUT_FILE"
