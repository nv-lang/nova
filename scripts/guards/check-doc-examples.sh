#!/usr/bin/env bash
# scripts/guards/check-doc-examples.sh — страж примеров кода в публикуемой
# доке: ловит ```nova-фрагменты, которые демонстрируют СНЯТЫЕ (retired/
# retracted/renamed) конструкции языка — формы, которые компилятор
# ГАРАНТИРОВАННО отвергает выделенной диагностикой (см. ниже «ИСТОЧНИК»).
#
# ПЛАН: docs/plans/242-doc-conventions-guard.md (семья стражей док-конвенций;
# соседи — check-doc-conventions.sh, check-doc-hygiene.sh). Смежное в реестре
# 221.1: №353 — снятая форма записи, оставшаяся рабочей в компиляторе, из-за
# чего примеры на ней и не ловились сборкой.
#
# ПОЧЕМУ. Примеры кода в docs/guide и spec/ никем машинно не проверяются —
# дока месяцами учила читателя писать снятый синтаксис (`let`, `readonly`,
# `external fn`, `addr_of(...)`, ...), и это вскрывалось только когда
# читатель/агент пробовал скопировать пример и получал compile error. Страж
# не компилирует фрагменты целиком (без `module`/`main` они не соберутся) —
# он ловит ЛЕКСИЧЕСКИЕ формы, для которых компилятор эмитит выделенный
# E_*_REMOVED/RETRACTED/RENAMED (или, для до-диагностической эпохи —
# trait/impl-for/throws — заведомо не существующий синтаксис) вместо
# generic parse error.
#
# ИСТОЧНИК ИСТИНЫ (грепнуто самим стражем-автором на дереве main
# 2026-08-05, коды и формулировки — из compiler-codegen/src/parser/mod.rs +
# compiler-codegen/src/types/mod.rs):
#   E_KW_REMOVED_LET              `let X = ...` / `if let` / `while let`
#                                 → канон ro/mut/consume (Plan 114, D184)
#   E_KW_REMOVED_READONLY         `readonly` (тип/параметр/поле)
#                                 → канон `ro` (Plan 114, D184)
#   E_REDUNDANT_POINTER_RO        `*ro T` → канон `*T` (Plan 147, D246)
#   E_UNSAFE_TYPE_MODIFIER_RENAMED `*unsafe T` → канон `*uninit T`
#                                 (НЕ трогает `*unsafe fn(...)` — легаси
#                                  fn-pointer форма, D216 §10 — unchanged)
#                                 (Plan 174.5, D216 amend)
#   E_REF_PARAM_FORM_REMOVED /
#   E_REF_CALL_MARKER_REMOVED     `(ref x T)`/`(mut ref x T)`/`(ro ref x T)`
#                                 в параметре, `f(ref x)` на call-site
#                                 → пишите без `ref` (Plan 184, D326-ревизия)
#   E_EXTERNAL_FN_RETRACTED       `external fn` → канон `extern "nova" fn` /
#                                 `extern "C" fn` (Plan 91.12, D282)
#   E_ADDR_OF_REMOVED             `addr_of(x)`/`addr_of_mut(x)` → канон `&x`
#                                 (Plan 118.6, D216 §4)
#   E_NULL_PTR_RETRACTED_USE_OPTION `null <прим.тип>` литерал → канон
#                                 `Option[*T]`/`(0 as *())` (Plan 118 Ф.5.7,
#                                 D214 amend)
#   E_PROTOCOL_RENAMED            #impl(Hashable|Equatable|Comparable|
#                                 Cloneable|Printable|DebugPrintable) →
#                                 канон Hash|Equal|Compare|Clone|Display|
#                                 Debug (Plan 137, D237)
#   постфиксный одиночный `!`     синтаксическая ошибка (нет такого
#                                 postfix-оператора в грамматике) — канон
#                                 постфикс-throw `!!` (Plan 19, C7, D85)
#   trait/impl-for/throws E       сняты задолго до диагностик-эпохи —
#                                 канон protocol/#impl(...)/Fail[E]
#
# ЧТО ПРОВЕРЯЕТ: файлы docs/guide/*.md (включает *.ru.md — та же маска),
# spec/*.md (включает *.en.md), README.md/README.ru.md — ТОЛЬКО верхний
# уровень этих каталогов (без рекурсии — так задание очертило периметр).
# Внутри каждого файла — ТОЛЬКО содержимое ```nova ... ``` блоков (другие
# языки — ```rust/```sh/```toml/... — не трогаются вовсе, даже если внутри
# встретится `let`).
#
# ГРАНУЛЯРНОСТЬ ИСКЛЮЧЕНИЯ — БЛОК, не строка (важная находка при вводе
# стража, docs/guide/typed-pointers.md): таблица «RETIRED form: / FINAL
# canonical equivalent:» размечает старую форму ЗАГОЛОВКОМ в первой строке
# блока, а конкретная старая форма (`*unsafe T`) появляется НИЖЕ, на строке
# БЕЗ маркера («//   was `*unsafe T`)» — построчный фильтр эту строку не
# исключил бы (маркер «RETIRED» — на другой строке того же блока), хотя
# семантически это ровно тот «явно учит не писать так» случай, что задание
# требует не считать нарушением. Поэтому: если ХОТЬ ОДНА строка внутри
# ```nova-блока содержит код диагностики `E_*` или слово
# retired/retracted/removed/снят (без учёта регистра) — исключается ВЕСЬ
# блок, не только эта строка. Компромисс осознанный: соседняя РЕАЛЬНАЯ
# ошибка в том же блоке была бы замаскирована — но риск ложного КРАСНОГО на
# легитимной «таблице снятых форм» выше и уже наблюдался на реальном дереве
# (typed-pointers.md/.ru.md), см. отчёт окна p-example-guard.
#
# Ratchet: scripts/guards/doc-examples.baseline, тот же формат key=N, что
# doc-hygiene/doc-conventions. Разные классы — разные ключи (не одна сумма),
# чтобы храповик показывал КАКОЙ класс просел/подрос, а не только общее число.
#
# ИСПОЛЬЗОВАНИЕ: check-doc-examples.sh [корень-репы]
# Выход: 0 — всё в пределах baseline; 1 — рост хотя бы одного класса (стдерр
# `DOC-EXAMPLES FAIL: ...`).
set -u
export LC_ALL=C
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
BASELINE="$SCRIPT_DIR/doc-examples.baseline"

fail=0
info() { echo "$1"; }
red() { echo "DOC-EXAMPLES FAIL: $1" >&2; fail=1; }

# ---------------------------------------------------------------------
# Извлечение ```nova-блоков с блок-гранулярным исключением «явно учит не
# писать так» (см. шапку). Печатает "file:lineno:content" для КАЖДОЙ
# сохранённой строки НЕ-исключённого блока — единый поток, дальше по нему
# гоняются независимые grep-классы. Работает В ПАМЯТИ (команда подстановка),
# без временных файлов — урок №321 (детерминизм на Windows/MSYS).
# ---------------------------------------------------------------------
# План 260 мера 5 (2026-08-11): ОДИН `awk` по ВСЕМУ списку файлов вместо
# процесса на каждый. Прежняя форма поднимала awk и sed на файл — на Windows
# порождение процесса дорого, и самотест стража доходил до 207 секунд, падая
# по сроку внутри гейта и оставаясь зелёным поодиночке (№558). Поднимать срок
# в третий раз значило бы признать, что гейт краснеет от загрузки машины.
#
# Состояние сбрасывается на границе файла (`FNR==1`): незакрытый ```nova-блок
# в конце файла НЕ должен протекать в следующий — прежняя форма получала это
# даром, потому что каждый файл шёл своим процессом.
extract_kept_nova_blocks_all() {  # file...
    awk '
        FNR == 1 { innova = 0; n = 0; exempt = 0 }
        /^```nova[ \t]*$/ { innova=1; n=0; exempt=0; next }
        /^```/ {
            if (innova && !exempt) {
                for (i = 1; i <= n; i++) print FILENAME ":" bufline[i] ":" buftext[i]
            }
            innova = 0
            next
        }
        innova {
            n++
            buftext[n] = $0
            bufline[n] = FNR
            low = tolower($0)
            if (low ~ /retired|retract|remov|снят/ || $0 ~ /E_[A-Z][A-Z0-9_]*/) exempt = 1
            next
        }
    ' "$@"
}

file_list=""
add_glob() {  # dir glob
    local d="$1" g="$2" f
    [ -d "$d" ] || return 0
    for f in "$d"/$g; do
        [ -f "$f" ] && file_list="$file_list $f"
    done
}
add_glob "$ROOT/docs/guide" '*.md'
add_glob "$ROOT/spec" '*.md'
[ -f "$ROOT/README.md" ] && file_list="$file_list $ROOT/README.md"
[ -f "$ROOT/README.ru.md" ] && file_list="$file_list $ROOT/README.ru.md"

# spec/open-questions.md — ИСКЛЮЧЕНИЕ ИЗ ПЕРИМЕТРА (найдено при вводе стража):
# файл сам себя объявляет «Что обсуждали, но не зафиксировали как решение»
# (заголовок) — журнал открытых/исторически закрытых design-вопросов, а не
# «как писать Nova сегодня» гайд. Он пестрит СВОЕГО ВРЕМЕНИ корректным
# синтаксисом (напр. `external fn` — было каноном ДО D282/Plan 91.12), не
# обновлённым при последующих ретракциях — по природе тот же класс, что
# `spec/decisions/` (внутренний рабочий норматив), которую
# check-doc-conventions.sh (dev_links) по той же причине исключает ИЗ
# ПЕРИМЕТРА, а не построчным маркером. Без исключения 32 из 112 находок
# `external fn` (28%) были бы шумом одного нечитательского файла, топящим
# сигнал по реально опубликованным docs/guide/*. См. летопись baseline.
file_list="$(printf '%s\n' $file_list | grep -v '/spec/open-questions\.md$')"

kept=""
files_scanned=0
if [ -n "$file_list" ]; then
    files_scanned=$(printf '%s\n' $file_list | wc -l | tr -d ' ')
    # shellcheck disable=SC2086  # список путей без пробелов — намеренное разбиение
    kept="$(extract_kept_nova_blocks_all $file_list)"
fi

# ---------------------------------------------------------------------
# Классы нарушений: id | E-код(ы) | regex (grep -E, применяется к "code"
# части строки — до содержимого; см. ниже per-класс замечания) | описание.
# ---------------------------------------------------------------------
# План 260, остаток меры 5 (2026-08-11): раньше каждый вызов поднимал свой
# `grep` над всем содержимым `kept`. Одиннадцать классов — одиннадцать
# процессов, и на Windows это пять секунд ДАЖЕ НА ПУСТОМ входе; самотест зовёт
# стража два десятка раз, отсюда его 234 секунды и ложное покраснение гейта под
# нагрузкой. Теперь содержимое кладётся на диск ОДИН раз, и `grep` читает файл,
# а не получает мегабайты через конвейер от `printf`.
#
# Числа обязаны остаться прежними — за этим следит неизменившийся самотест.
# Файл создаётся ОДИН раз здесь, на верхнем уровне, а не лениво внутри
# подстановки команды: `$( … )` выполняется в ПОДОБОЛОЧКЕ, и зарегистрированный
# там `trap … EXIT` срабатывает при её завершении — файл исчезал ровно в тот
# момент, когда путь к нему возвращался наружу. Поймано первым же прогоном:
# страж сказал «значение не целое — страж сломан», то есть повёл себя так, как
# мы и требуем от проверок (№475: шаг, который не смог, обязан отказать).
KEPT_FILE="${TMPDIR:-/tmp}/doc_examples_kept_$$"
trap 'rm -f "$KEPT_FILE"' EXIT
if [ -n "$kept" ]; then
    printf '%s\n' "$kept" > "$KEPT_FILE"
else
    : > "$KEPT_FILE"
fi

count_class() {  # regex
    if [ -z "$kept" ]; then
        echo 0
        return
    fi
    grep -cE "$1" "$KEPT_FILE" 2>/dev/null
}
list_class() {  # regex
    [ -z "$kept" ] && return
    grep -E "$1" "$KEPT_FILE" 2>/dev/null
}

# 1. `let X = expr` (объявление) + `if let` / `while let` — E_KW_REMOVED_LET.
#    Требуем идентификатор+`=` после `let`, чтобы не ловить прозу «let's»
#    внутри `//`-комментариев (апостроф — не [[:space:]], паттерн не
#    совпадёт). `(^|[^A-Za-z0-9_])let` вместо `\<let\>` — переносимость
#    (не весь grep одинаково трактует GNU `\<`/`\>` word-boundary escapes).
re_let='(^|[^A-Za-z0-9_])let[[:space:]]+[A-Za-z_][A-Za-z0-9_]*[[:space:]]*=|(^|[^A-Za-z0-9_])(if|while)[[:space:]]+let([^A-Za-z0-9_]|$)'
n_let=$(count_class "$re_let")

# 2. `readonly` keyword — E_KW_REMOVED_READONLY.
re_readonly='\<readonly\>'
n_readonly=$(count_class "$re_readonly")

# 3. `*ro T` (pointee-position) — E_REDUNDANT_POINTER_RO.
re_ptr_ro='\*ro[[:space:]]+[A-Za-z_]'
n_ptr_ro=$(count_class "$re_ptr_ro")

# 4. `*unsafe T` (T ≠ `fn(`) — E_UNSAFE_TYPE_MODIFIER_RENAMED. Легаси
#    fn-pointer форма `*unsafe fn(...)` ОСТАЁТСЯ валидной (D216 §10) —
#    исключаем её отдельным grep -v.
re_unsafe_ptr='\*unsafe[[:space:]]+[A-Za-z_]'
re_unsafe_ptr_fn_ok='\*unsafe[[:space:]]+fn[[:space:]]*\('
n_unsafe_ptr=0
if [ -n "$kept" ]; then
    n_unsafe_ptr=$(printf '%s\n' "$kept" | grep -E "$re_unsafe_ptr" 2>/dev/null | grep -cvE "$re_unsafe_ptr_fn_ok" 2>/dev/null)
fi

# 5. Постфиксный одиночный `!` (не `!!`) — синтаксическая ошибка, канон
#    `!!`. Проверяем только КОДОВУЮ часть строки (до первого `//`), чтобы
#    не ловить эмфазу в комментариях («// no Fail[E]!») — реальный ложняк,
#    пойманный на cleanup-cookbook.md при вводе стража.
n_bang=0
bang_matches=""
if [ -n "$kept" ]; then
    bang_matches="$(printf '%s\n' "$kept" | awk -F'//' '{
        code = $1
        sub(/[ \t]+$/, "", code)
        if (code ~ /[A-Za-z0-9_)\]][!][[:space:]]*$/ && code !~ /!![[:space:]]*$/) print
    }')"
    [ -n "$bang_matches" ] && n_bang=$(printf '%s\n' "$bang_matches" | grep -c '')
fi

# 6. trait-блок / impl-for-блок / `throws E` — сняты задолго до
#    диагностик-эпохи (протокол/#impl(...)/Fail[E] — канон).
re_trait_impl_throws='^[^:]*:[0-9]+:[[:space:]]*trait[[:space:]]+[A-Za-z_][A-Za-z0-9_]*.*\{|^[^:]*:[0-9]+:[[:space:]]*impl[[:space:]]+[A-Za-z_][A-Za-z0-9_]*(\[[^]]*\])?[[:space:]]+for[[:space:]]+[A-Za-z_]|\<fn[[:space:]]+[A-Za-z_][A-Za-z0-9_]*\([^)]*\)[[:space:]]+throws[[:space:]]+[A-Za-z_]'
n_trait_impl_throws=$(count_class "$re_trait_impl_throws")

# 7. `ref` — снятая форма параметра/call-site — E_REF_PARAM_FORM_REMOVED /
#    E_REF_CALL_MARKER_REMOVED. И параметр (`(ref x T)`/`(mut ref x T)`/
#    `(ro ref x T)`), и call-site (`f(ref x)`) начинаются с `(` сразу за
#    которой (опционально после mut/ro) идёт `ref`.
re_ref='\([[:space:]]*(mut[[:space:]]+|ro[[:space:]]+)?ref[[:space:]]+[A-Za-z_]'
n_ref=$(count_class "$re_ref")

# 8. `external fn` / `external unsafe fn` — E_EXTERNAL_FN_RETRACTED.
re_external_fn='\<external[[:space:]]+(unsafe[[:space:]]+)?fn\>'
n_external_fn=$(count_class "$re_external_fn")

# 9. `addr_of(...)` / `addr_of_mut(...)` — E_ADDR_OF_REMOVED.
re_addr_of='\<addr_of(_mut)?[[:space:]]*\('
n_addr_of=$(count_class "$re_addr_of")

# 10. `null <прим.тип>` литерал — E_NULL_PTR_RETRACTED_USE_OPTION.
re_null_ptr='\<null[[:space:]]+(ptr|int|i8|i16|i32|i64|u8|u16|u32|u64|uint|f32|f64|bool|char|str)\>'
n_null_ptr=$(count_class "$re_null_ptr")

# 11. `#impl(<старое-имя-протокола>)` — E_PROTOCOL_RENAMED (Plan 137, D237).
re_protocol_renamed='#impl\([[:space:]]*(Hashable|Equatable|Comparable|Cloneable|Printable|DebugPrintable)\>'
n_protocol_renamed=$(count_class "$re_protocol_renamed")

# ---------------------------------------------------------------------
# Ratchet-хелпер (образец: check-doc-conventions.sh ratchet_check).
# ---------------------------------------------------------------------
ratchet_check() {  # key current_value description
    local key="$1" cur="$2" desc="$3" base
    base=$(grep -E "^$key=[0-9]+[[:space:]]*$" "$BASELINE" 2>/dev/null | tail -1 | cut -d= -f2 | tr -d '[:space:]')
    if [ -z "$base" ]; then
        red "$key: в $BASELINE нет строки '$key=<целое>' (страж не может ratchet-ить без базы)"
        return
    fi
    case "$cur" in ''|*[!0-9]*)
        red "$key: текущее значение '$cur' не целое — страж сломан, чинить скрипт"
        return ;;
    esac
    if [ "$cur" -gt "$base" ]; then
        red "$key=$cur > baseline=$base ($desc; рост запрещён без письменной правки baseline в этом же коммите)"
    else
        info "doc-examples ok: $key=$cur <= baseline=$base ($desc)"
    fi
}

info "doc-examples: просканировано файлов = $files_scanned (docs/guide/*.md + spec/*.md + README*.md)"

ratchet_check retired_kw_let               "$n_let"               "\`let\`/\`if let\`/\`while let\` (E_KW_REMOVED_LET)"
ratchet_check retired_kw_readonly          "$n_readonly"          "\`readonly\` (E_KW_REMOVED_READONLY)"
ratchet_check retired_pointer_ro           "$n_ptr_ro"            "\`*ro T\` (E_REDUNDANT_POINTER_RO)"
ratchet_check retired_unsafe_type_modifier "$n_unsafe_ptr"        "\`*unsafe T\` (E_UNSAFE_TYPE_MODIFIER_RENAMED)"
ratchet_check retired_postfix_bang         "$n_bang"              "постфиксный одиночный \`!\` (канон \`!!\`)"
ratchet_check retired_trait_impl_throws    "$n_trait_impl_throws" "trait/impl-for/throws E (canon protocol/#impl/Fail[E])"
ratchet_check retired_ref_form             "$n_ref"               "\`ref\` в параметре/call-site (E_REF_PARAM_FORM_REMOVED/E_REF_CALL_MARKER_REMOVED)"
ratchet_check retired_external_fn          "$n_external_fn"       "\`external fn\` (E_EXTERNAL_FN_RETRACTED)"
ratchet_check retired_addr_of              "$n_addr_of"           "\`addr_of\`/\`addr_of_mut\` (E_ADDR_OF_REMOVED)"
ratchet_check retired_null_ptr             "$n_null_ptr"          "\`null <тип>\` литерал (E_NULL_PTR_RETRACTED_USE_OPTION)"
ratchet_check retired_protocol_renamed     "$n_protocol_renamed"  "\`#impl(<старое имя протокола>)\` (E_PROTOCOL_RENAMED)"

# Печатаем сами находки (не только счёт) — полезно и при первом замере, и
# при локальном прогоне после правки доки, до коммита baseline.
if [ "$fail" -eq 0 ] && [ "${DOC_EXAMPLES_SHOW_MATCHES:-1}" = "1" ]; then
    for pair in "retired_kw_let:$re_let" "retired_kw_readonly:$re_readonly" \
                "retired_pointer_ro:$re_ptr_ro" "retired_trait_impl_throws:$re_trait_impl_throws" \
                "retired_ref_form:$re_ref" "retired_external_fn:$re_external_fn" \
                "retired_addr_of:$re_addr_of" "retired_null_ptr:$re_null_ptr" \
                "retired_protocol_renamed:$re_protocol_renamed"; do
        key="${pair%%:*}"
        re="${pair#*:}"
        m="$(list_class "$re")"
        [ -n "$m" ] && printf 'doc-examples находки (%s):\n%s\n' "$key" "$m"
    done
    if [ -n "$kept" ]; then
        m="$(printf '%s\n' "$kept" | grep -E "$re_unsafe_ptr" 2>/dev/null | grep -vE "$re_unsafe_ptr_fn_ok" 2>/dev/null)"
        [ -n "$m" ] && printf 'doc-examples находки (retired_unsafe_type_modifier):\n%s\n' "$m"
    fi
    [ -n "$bang_matches" ] && printf 'doc-examples находки (retired_postfix_bang):\n%s\n' "$bang_matches"
fi


# ---------------------------------------------------------------------
# ПРЕДУПРЕЖДЕНИЕ (не храповик): методы-аналоги операторов в примерах.
# Конвенция (nv-coding-style §"операторы", D46): в коде пишут `a + b`, а
# `@plus`/`@minus`/`@times`/`@neg` — это ИМЕНА ПЕРЕГРУЗКИ. Прецедент:
# документация bignum учила `a.plus(b)` (замечание владельца 2026-08-05).
# Не красный: у части типов операторной формы нет (BigFloat требует контекст
# точности), поэтому это подсказка для вычитки, а не гейт.
op_style=$(printf '%s
' "$kept" | grep -oE '\.(plus|minus|times|neg)\(' | wc -l)
if [ "${op_style:-0}" -gt 0 ]; then
    info "doc-examples ПРЕДУПРЕЖДЕНИЕ (operator_style): вызовов .plus/.minus/.times/.neg в примерах — $op_style; где у типа есть операторная форма, писать \`a + b\` (конвенция); гейт не роняется"
fi


# ---------------------------------------------------------------------
# ИНФОРМАЦИОННО: spec/decisions/** — вне гейта, но на сайте публикуется.
# D-блоки цитируют снятые формы ЗАКОННО (история решений, «было → стало»),
# поэтому красный гейт здесь утонул бы в шуме; держим видимость числом.
# Решение по хэндоффу 2026-08-05, п.5: периметр не расширяем, помечаем
# архивным жанром и показываем счётчик.
# ---------------------------------------------------------------------
if [ -d "$ROOT/spec/decisions" ]; then
    dec_retired=$(awk '/^```nova/{f=1; next} /^```/{f=0; next} f' "$ROOT"/spec/decisions/*.md 2>/dev/null         | grep -cE '(^|[^A-Za-z_])(let [a-z_]|external fn|addr_of\(|readonly [a-z_])' )
    info "doc-examples ИНФОРМАЦИОННО (decisions_retired): снятых форм в nova-примерах spec/decisions/** — ${dec_retired:-0}; это исторический норматив, гейт не применяется (см. шапку)"
fi

exit $fail
