#!/bin/sh
# scripts/guards/selftest/test-check-novac-table-one-filler.sh — самотест стража
# «у таблицы реестра novac ОДИН наполнитель» (план 274.5 §3-пред62).
#
# ПОЧЕМУ САМОТЕСТ ИМЕННО ТАКОЙ. Страж заведён по классу, который никогда не
# приходит отказом: таблицу наполняет один модуль, спрашивает другой, и ответ
# «нет такого» ВЕРЕН — неверен вход. Значит доказывать надо обе стороны:
# что один наполнитель проходит, а второй краснит С АДРЕСАМИ обоих мест;
# и отдельно — что ноль подсудных мест это КРАСНЫЙ, а не «чисто».
#
# СЕМЬ СЛУЧАЕВ, каждый отвечает на свой вопрос:
#   1. один модуль-наполнитель, число на базе — зелёный;
#   2. ГЛАВНЫЙ: два модуля пишут в одну таблицу — КРАСНЫЙ, и названы ОБА места;
#   3. ноль мест записи — КРАСНЫЙ как потеря мишени, а не зелёный ноль;
#   4. нет объявления `export type Ctx` — КРАСНЫЙ: имена таблиц читаются оттуда,
#      и без него страж считал бы пустоту;
#   5. рост общего числа мест над базой — КРАСНЫЙ;
#   6. таблица из базы исчезла из `Ctx` — КРАСНЫЙ: база обязана ехать за полем;
#   7. запись, ПРОЦИТИРОВАННАЯ в комментарии, не считается (проза — не код).
#
# Фикстуры свои, настоящее дерево не читается ни в одном случае.
set -u
LC_ALL=C; export LC_ALL

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-novac-table-one-filler.py"
T="${TMPDIR:-/tmp}/novac-table-one-filler-selftest.$$"
FAILED=0

ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; FAILED=$((FAILED+1)); }

mkdir -p "$T"
trap 'rm -rf "$T"' EXIT

# Мини-novac: объявление Ctx с двумя таблицами.
mk_ctx() {  # $1 = каталог-src
    mkdir -p "$1/sem"
    { echo 'module novac.sem'
      echo ''
      echo 'export type Ctx {'
      echo '    defs DefTable /// THE unified top-level name registry'
      echo '    fns FnTable /// THE callable registry'
      echo '}'; } > "$1/sem/sem.nv"
}

# Наполнитель в модуле sem: $2 записей в `defs`.
mk_filler_sem() {  # $1 = каталог-src, $2 = 1 или 2
    mkdir -p "$1/sem"
    { echo 'module novac.sem'
      echo 'fn collect(mut defs DefTable) -> () {'
      echo '    defs.add("a", DefTarget.DefType(id))'
      if [ "$2" -ge 2 ]; then echo '    defs.add("b", DefTarget.DefType(id))'; fi
      echo '}'; } > "$1/sem/collect.nv"
}

run() {  # $1 = каталог-src, $2 = файл базы
    NOVAC_TABLE_FILLERS_BASELINE="$2" python "$G" "$ROOT" "$1" >"$T/out" 2>"$T/err"
}

printf 'defs=1:sem\ntotal=1\n' > "$T/base1"

# --- 1. один наполнитель, на базе ------------------------------------------------
mk_ctx "$T/one"; mk_filler_sem "$T/one" 1
mkdir -p "$T/one/check"
printf 'module novac.check\nfn judge(ctx Ctx) -> bool => ctx.defs.has("a")\n' > "$T/one/check/decls.nv"
if run "$T/one" "$T/base1"; then
    ok "один модуль-наполнитель при базе 1 — зелёный (чтение из check не считается записью)"
else
    bad "чистая подложка, а страж красный: $(cat "$T/err")"
fi

# --- 2. ГЛАВНЫЙ: два модуля пишут в одну таблицу --------------------------------
mk_ctx "$T/two"; mk_filler_sem "$T/two" 1
mkdir -p "$T/two/check"
{ echo 'module novac.check'
  echo 'fn judge(mut ctx Ctx) -> () {'
  echo '    ctx.defs.add("b", DefTarget.DefType(id))'
  echo '}'; } > "$T/two/check/decls.nv"
if run "$T/two" "$T/base1"; then
    bad "два модуля-наполнителя одной таблицы прошли зелёным"
else
    if grep -q 'sem/collect.nv:3' "$T/err" && grep -q 'check/decls.nv:3' "$T/err"; then
        ok "два наполнителя — красный, названы ОБА места файл:строка"
    else
        bad "красный, но адреса обоих мест не названы: $(cat "$T/err")"
    fi
fi

# --- 3. мишень потеряна: ноль мест записи ---------------------------------------
mk_ctx "$T/nosites"
mkdir -p "$T/nosites/check"
printf 'module novac.check\nfn judge(ctx Ctx) -> bool => ctx.defs.has("a")\n' > "$T/nosites/check/decls.nv"
if run "$T/nosites" "$T/base1"; then
    bad "ноль мест записи — а страж зелёный"
else
    grep -q 'мишень' "$T/err" && ok "ноль мест записи — красный, назван потерей мишени" \
        || bad "красный, но не про мишень: $(cat "$T/err")"
fi

# --- 4. мишень потеряна: нет объявления Ctx -------------------------------------
mkdir -p "$T/noctx/sem"
printf 'module novac.sem\nfn nothing() -> int => 1\n' > "$T/noctx/sem/sem.nv"
mk_filler_sem "$T/noctx" 1
if run "$T/noctx" "$T/base1"; then
    bad "объявления Ctx нет — а страж зелёный"
else
    grep -q 'мишень' "$T/err" && ok "нет объявления Ctx — красный, назван потерей мишени" \
        || bad "красный, но не про мишень: $(cat "$T/err")"
fi

# --- 5. рост над базой -----------------------------------------------------------
mk_ctx "$T/grow"; mk_filler_sem "$T/grow" 2
if run "$T/grow" "$T/base1"; then
    bad "два места записи при базе 1 прошли зелёным"
else
    grep -q 'total=2' "$T/err" && ok "рост над базой — красный, новое состояние напечатано построчно" \
        || bad "красный, но без сегодняшнего состояния: $(cat "$T/err")"
fi

# --- 6. таблица из базы исчезла из Ctx ------------------------------------------
mk_ctx "$T/gone"; mk_filler_sem "$T/gone" 1
printf 'defs=1:sem\nvariant_rows=0:sem\ntotal=1\n' > "$T/base_gone"
if run "$T/gone" "$T/base_gone"; then
    bad "база называет таблицу, которой в Ctx нет, — а страж зелёный"
else
    grep -q 'variant_rows' "$T/err" && ok "исчезнувшая из Ctx таблица базы — красный, названа поимённо" \
        || bad "красный, но имя исчезнувшей таблицы не названо: $(cat "$T/err")"
fi

# --- 7. проза не считается -------------------------------------------------------
mk_ctx "$T/prose"; mk_filler_sem "$T/prose" 1
mkdir -p "$T/prose/check"
{ echo 'module novac.check'
  echo '// the old way was ctx.defs.add(name, target) and it moved to sem'
  echo 'fn judge(ctx Ctx) -> bool => ctx.defs.has("a")'; } > "$T/prose/check/decls.nv"
if run "$T/prose" "$T/base1"; then
    ok "запись, процитированная в комментарии, не считается наполнителем"
else
    bad "комментарий посчитан как код: $(cat "$T/err")"
fi

echo "итог: FAIL $FAILED"
if [ "$FAILED" -eq 0 ]; then
    echo "test-check-novac-table-one-filler ok: второй наполнитель, рост и обе потери мишени краснеют с адресами; один наполнитель и проза законны"
    exit 0
fi
exit 1
