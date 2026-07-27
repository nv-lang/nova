#!/bin/sh
# scripts/arch-ratchet.sh — архитектурный храповик легаси-кодогена emit_c.rs.
#
# ПОЧЕМУ. План 196 «Одна правда» требует, чтобы новая семантика шла через
# чекер-канал (resolved_*), а не наращивала per-синтаксис-позицию разбор в
# `compiler-codegen/src/codegen/emit_c.rs` — но это правило жило только в
# конвенциях/брифах, соблюдение зависело от памяти интегратора. План 231
# трек Е (docs/plans/231-bug-cycle-exit.md, «вопрос владельца 2026-07-26:
# 196 не соблюдался; как обеспечить?») + исполнительный дом
# docs/plans/231.2-enforcement-infra.md §1.
#
# ЧТО ПРОВЕРЯЕТ. Две метрики emit_c.rs НЕ МОГУТ РАСТИ относительно
# baseline (снижение — приветствуется, тогда тоже обнови baseline вниз):
#   lines — `wc -l` всего файла;
#   infer  — число вызовов `infer_expr_c_type` (собственный инференс типов
#            прямо из эмиссии, вместо чтения готового резолва из чекера).
# Baseline хранится в scripts/arch-ratchet.baseline как `key=значение`
# построчно (плюс `#`-комментарии — история решений, зачем росло).
#
# КАК ОСОЗНАННО РАСТИ. Рост запрещён МОЛЧА. Если конкретная волна
# действительно должна вырастить emit_c.rs (напр. новый ClosureFull-арм,
# симметричный существующему ClosureLight — см. запись 2026-07-26 в самом
# baseline-файле как образец), правь `scripts/arch-ratchet.baseline` В ТОМ
# ЖЕ коммите: новое число + `#`-абзац с обоснованием (что именно выросло,
# почему это emit-слой, а не канал, какие репро/CU-прогоны верифицировали
# фикс). Baseline-диф в ревью — явный красный флаг «здесь легаси вырос»,
# который обязан быть объяснён текстом рядом.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/arch-ratchet.sh
# Запускается из scripts/gate.sh (шаг «arch-ratchet», до cargo build —
# дёшево и быстро). Выход: 0 — метрики ≤ baseline, 1 — рост без правки
# baseline (печатает какая метрика и на сколько).
set -u
BASE_FILE="$(dirname "$0")/arch-ratchet.baseline"
EMIT="compiler-codegen/src/codegen/emit_c.rs"

m_lines=$(wc -l < "$EMIT" | tr -d ' ')
m_infer=$(grep -c "infer_expr_c_type" "$EMIT")
fail=0
while IFS='=' read -r key base; do
  case "$key" in \#*|'') continue;; esac
  cur=$(eval echo "\$m_$key")
  if [ "$cur" -gt "$base" ]; then
    echo "ARCH-RATCHET FAIL: $key=$cur > baseline=$base (emit_c must not grow; fix belongs to checker channel / IR path)" >&2
    fail=1
  else
    echo "arch-ratchet ok: $key=$cur <= $base"
  fi
done < "$BASE_FILE"
exit $fail
