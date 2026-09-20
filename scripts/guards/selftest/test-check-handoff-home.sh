#!/usr/bin/env bash
# Самотест check-handoff-home: страж обязан КРАСНЕТЬ по каждой из двух своих
# сторон ПО ОТДЕЛЬНОСТИ и зеленеть на здоровом дереве.
#
# Почему обе стороны проверяются порознь: страж, у которого срабатывает только
# одна половина, выглядит рабочим ровно до того дня, когда нарушат вторую.
set -u

ROOT="${1:-.}"
GUARD="$ROOT/scripts/guards/check-handoff-home.py"
NAME="test-check-handoff-home"
FAILED=0

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mk_tree() {
    # Здоровое дерево: команда называет дом, свежих передач в игноре нет.
    local d="$1"
    mkdir -p "$d/.claude/commands" "$d/docs/.sessions" "$d/scripts/guards"
    printf '4. **Записать передачу в СВОЮ РОЛЕВУЮ ЗАПИСКУ** ...\n' \
        > "$d/.claude/commands/stop.md"
}

# ── 1. здоровое дерево — PASS ────────────────────────────────────────────
mk_tree "$TMP/ok"
if python "$GUARD" "$TMP/ok" >/dev/null 2>&1; then
    echo "$NAME: ok — здоровое дерево принято"
else
    echo "$NAME: FAIL — здоровое дерево отвергнуто" >&2
    FAILED=1
fi

# ── 2. свежая передача в игнорируемом каталоге — FAIL ────────────────────
mk_tree "$TMP/stale"
printf '# peredacha\n' > "$TMP/stale/docs/.sessions/handoff-deadbeef.md"
# mtime = сейчас, то есть заведомо после даты правила.
if python "$GUARD" "$TMP/stale" >/dev/null 2>&1; then
    echo "$NAME: FAIL — свежая передача в docs/.sessions НЕ поймана" >&2
    FAILED=1
else
    echo "$NAME: ok — свежая передача в игнорируемом каталоге отвергнута"
fi

# ── 3. СТАРЫЙ файл там же — PASS (история не судится) ────────────────────
mk_tree "$TMP/old"
printf '# staraya peredacha\n' > "$TMP/old/docs/.sessions/handoff-old.md"
# 2026-09-01 — заведомо раньше даты правила (2026-09-18).
touch -t 202609010000 "$TMP/old/docs/.sessions/handoff-old.md" 2>/dev/null \
    || touch -d '2026-09-01' "$TMP/old/docs/.sessions/handoff-old.md"
if python "$GUARD" "$TMP/old" >/dev/null 2>&1; then
    echo "$NAME: ok — старый файл не судится"
else
    echo "$NAME: FAIL — старый файл покраснел, хотя он история" >&2
    FAILED=1
fi

# ── 4. правило вынуто из /stop — FAIL ────────────────────────────────────
mk_tree "$TMP/mute"
printf '4. Zapisat peredachu kuda-nibud.\n' > "$TMP/mute/.claude/commands/stop.md"
if python "$GUARD" "$TMP/mute" >/dev/null 2>&1; then
    echo "$NAME: FAIL — команда без предписания НЕ поймана" >&2
    FAILED=1
else
    echo "$NAME: ok — команда без предписания отвергнута"
fi

# ── 5. ДОКАЗАТЕЛЬСТВО С ПАРОЙ — PASS (НОВОЕ поведение, 2026-09-20) ─────
# Зачем клетка: с этого дня файл в docs/.sessions/ ТРЕБУЕТСЯ Stop-хуком как
# доказательство остановки, и краснеть на его НАЛИЧИИ стало неверно.
# Клетка 4 ниже держит вторую половину: без пары отказ остался.
mk_tree "$TMP/paired"
mkdir -p "$TMP/paired/docs/dev/prompts"
printf '# peredacha\n' > "$TMP/paired/docs/.sessions/handoff-beef0001.md"
printf '## 0-2026-09-20-den. ОСТАНОВКА smeny\n\ntelo\n' \
    > "$TMP/paired/docs/dev/prompts/integrator-handoff.md"
if python "$GUARD" "$TMP/paired" >/dev/null 2>&1; then
    echo "$NAME: ok — доказательство с парой в записке принято"
else
    echo "$NAME: FAIL — доказательство с парой отвергнуто" >&2
    FAILED=1
fi

# ── 6. ПАРА БЕЗ СЛОВА ОСТАНОВКА — FAIL (контроль к клетке 5) ───────
# Без этой клетки пятая доказывала бы только что ЗАПИСКА СУЩЕСТВУЕТ,
# а не что в ней есть РАЗДЕЛ ОСТАНОВКИ: замена заголовка делает
# причину приёма невозможной, и в этом смысл контроля.
mk_tree "$TMP/nopair"
mkdir -p "$TMP/nopair/docs/dev/prompts"
printf '# peredacha\n' > "$TMP/nopair/docs/.sessions/handoff-beef0002.md"
printf '## 0-2026-09-20-den. Sostoyanie smeny\n\ntelo\n' \
    > "$TMP/nopair/docs/dev/prompts/integrator-handoff.md"
if python "$GUARD" "$TMP/nopair" >/dev/null 2>&1; then
    echo "$NAME: FAIL — записка без раздела ОСТАНОВКА засчитана за пару" >&2
    FAILED=1
else
    echo "$NAME: ok — записка без раздела ОСТАНОВКА парой не считается"
fi

if [ "$FAILED" -ne 0 ]; then
    echo "$NAME: FAIL" >&2
    exit 1
fi
echo "$NAME ok: обе стороны краснеют порознь, здоровое дерево и история зелены"
