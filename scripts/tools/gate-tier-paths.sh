#!/usr/bin/env bash
# scripts/tools/gate-tier-paths.sh — ОДИН дом правила «какие пути судит какой ярус».
#
# ЗАЧЕМ ОТДЕЛЬНЫМ ФАЙЛОМ. Правило «эти пути принадлежат ярусу novac» уже жило в
# дереве ДВАЖДЫ одной и той же регуляркой:
#   scripts/tools/push-after-gate.sh   — выбор яруса перед пушем;
#   scripts/tools/git-readiness.sh     — отчёт о готовности.
# Реестр 221.1 №988 потребовал спросить то же самое ТРЕТЬЕМУ потребителю —
# `scripts/guards/check-merge-discipline.sh`, на пути слияния. Третья копия
# регулярки разошлась бы с первыми двумя на первой же правке (новый каталог
# novac-стражей — и один потребитель его видит, другой нет), поэтому правило
# вынесено сюда, а копии заменены вызовом.
#
# ЧТО ЭТО НЕ ДЕЛАЕТ. Здесь нет выбора «loop/push» для основного гейта — он
# зависит от вида изменённых исходников и живёт там, где применяется
# (`push-after-gate.sh`). Здесь только принадлежность путей ярусу novac.
#
# ИСПОЛЬЗОВАНИЕ:
#   . "$(dirname "$0")/../tools/gate-tier-paths.sh"
#   printf '%s\n' "$CHANGED" | novac_paths_count      # -> число путей яруса novac
#   novac_paths_re                                    # -> сама регулярка, для сообщений

# Пути, за которые отвечает ЯРУС NOVAC (`scripts/gate-novac.sh`, 87 стражей
# `check-novac-*`), и которых основной `scripts/gate.sh` не судит вовсе.
NOVAC_TIER_PATH_RE='^(novac/|scripts/gate-novac\.sh|scripts/guards/check-novac-|scripts/guards/novac-)'

novac_paths_re() { printf '%s' "$NOVAC_TIER_PATH_RE"; }

# Читает пути со stdin (по одному в строке), печатает их число.
# `|| true`: grep -c возвращает 1 на нуле совпадений, а ноль здесь — законный
# ответ, а не отказ.
novac_paths_count() {
    grep -c -E "$NOVAC_TIER_PATH_RE" || true
}
