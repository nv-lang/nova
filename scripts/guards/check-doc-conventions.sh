#!/usr/bin/env bash
# scripts/guards/check-doc-conventions.sh — страж конвенции документации
# docs/dev/doc-conventions.md (Plan 242 «enforcement doc-conventions», подопечная
# конвенция ссылается на этот страж в своей строке `enforcement:`; контекст
# пар en/ru — Plan 241 «двуязычная дока ВЕЗДЕ»).
#
# ПОЧЕМУ. doc-conventions.md фиксировал языковую политику/парность/дрейф
# текстом, без машины — соблюдение зависело от памяти интегратора (тот же
# класс дыры, что 196/231 §4в для кода: «в скрипте/правиле нет толку, если
# не подключён к автопроверке»). Plan 242 переводит пять правил в машину.
#
# ЧТО ПРОВЕРЯЕТ (шесть независимых проверок; LC_ALL=C — байтовый grep,
# урок msys2 2026-07-31: не-ASCII без LC_ALL=C молча даёт 0 хитов):
#
#   1. spec_en_header  — у КАЖДОГО английского `spec/X.md`, для которого существует
#      ru-оригинал `spec/X.ru.md` (это и есть перевод-пара; пара теперь обязательна
#      для всех публичных страниц — см. `check-doc-language-pairs.sh`, реестр №8, №9),
#      обязаны быть в первых 15 строках: точная фраза «Informative
#      translation; the Russian text is normative.» + `source_rev:` +
#      `source_date: YYYY-MM-DD`. До первого перевода — 0 пар — вакуумно-
#      зелено (план 242 п.1).
#
#   2. guide_pairing   — читает `docs/guide/PUBLISHED.list` (канон
#      публикации, план 241-Ф.1b/242-ревизия п.2): одно имя без расширения
#      на строку (`#`-комментарии и пустые строки игнорируются). Для
#      каждого имени обязаны существовать И `имя.md`, И `имя.ru.md` —
#      отсутствие любой стороны красное. Файла ещё нет → вакуумно-зелено
#      (план вводит его в 241-Ф.1b).
#
#   2b. guide_same_commit — best-effort: правка одной стороны пары без
#      правки другой В ТОМ ЖЕ диапазоне коммитов — красный (допуск:
#      диапазон правит ТОЛЬКО `source_rev:`/`source_date:`-строки).
#      Диапазон передаётся ВТОРЫМ аргументом или `DOC_GUARD_DIFF_BASE`;
#      без него — пропуск с пометкой (одинокий снимок дерева не несёт
#      истории правок, гейт это не проверка, а diff-инструмент — CI job
#      передаёт диапазон явно, см. .github/workflows/nova-gate.yml).
#
#   3. plan_status (ratchet) — `docs/plans/NNN-*.md` (тот же фильтр имён,
#      что `scripts/tools/gen-plan-status.sh`: без README.md/STATUS.md,
#      без суффиксов -notes/-progress/-execution-plan/-session*/-history, только
#      файлы с ведущей цифрой) обязаны нести строку `**Статус:**`.
#      Огромный исторический долг (458 из 599 на момент введения стража —
#      старые планы до появления самой конвенции) — не ретрофитить разом,
#      поэтому число НЕПОКРЫТЫХ файлов — храповик «только вниз», как
#      lines/infer в arch-ratchet, а не жёсткий 0.
#
#   4. dev_links (ratchet) — число упоминаний `docs/dev/` (оба вида: явный
#      путь `docs/dev/...` И относительный `../dev/...`/`../docs/dev/...`,
#      которым реально пользуются guide/spec-файлы) в `docs/guide/**` и
#      `spec/**` (включая `spec/decisions/`) — `docs/dev/` никогда не
#      публикуется (`#publishing`), рост числа ссылок = растущий риск
#      битой/недоступной ссылки на сайте. Храповик, база — файлом.
#
#   5. code_block_identity (ratchet) — для КАЖДОЙ обнаруженной пары
#      (guide: `X.md`/`X.ru.md` по факту наличия обеих сторон, вне
#      зависимости от PUBLISHED.list; spec: `X.md`/`X.ru.md`) код-блоки
#      (```-фенсы по порядку) обязаны быть байт-в-байт идентичны
#      (`#compilable-examples`/`#translation-drift`: «код-примеры
#      переносятся байт-в-байт»). НАЙДЕННЫЙ ДОЛГ на момент введения
#      стража: все 3 существующих guide-пары (channels/contracts/
#      nova-cli) переводят КОММЕНТАРИИ ВНУТРИ код-блоков — это
#      предшествует правилу байт-идентичности (241-ревизия 2026-08-03) и
#      уже нарушает существующую норму `#translation-drift`. Заморожено
#      храповиком (3), не ретрофитится этой волной — см. отчёт окна p242.
#
# КРОСС-РЕПНОСТЬ (план 242 §2b): скрипт принимает корень ЛЮБОЙ репы первым
# аргументом и не считает отсутствие `docs/plans/`, `spec/` или
# `docs/guide/` ошибкой — соответствующая проверка просто вакуумно-зелена
# (пакетные репы вроде nova-http несут только README-пары, без spec/plans).
# Baseline читается ИЗ РЕПЫ САМОГО СКРИПТА (`scripts/guards/` в nova), а не
# из проверяемого корня — единственный источник ratchet-баз общий для всех
# репозиториев, вызывающих этот скрипт по соседнему пути (см. план §2b,
# рубеж 1 — pre-commit пакетной репы зовёт `../nova/scripts/guards/…`).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-doc-conventions.sh [корень-репы] [diff-base]
# Выход: 0 — все проверки в норме; 1 — есть нарушение (сообщения с именами
# файлов на stderr, `DOC-CONVENTIONS FAIL: ...`).
set -u
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
ROOT="${1:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
DIFF_BASE="${2:-${DOC_GUARD_DIFF_BASE:-}}"
BASELINE="$SCRIPT_DIR/doc-conventions.baseline"

fail=0
info() { echo "$1"; }
red() { echo "DOC-CONVENTIONS FAIL: $1" >&2; fail=1; }

# ---------------------------------------------------------------------
# 1. английские spec/*.md — шапка + frontmatter (только реальные переводы, т.е.
#    файлы, у которых есть ru-оригинал spec/X.md).
# ---------------------------------------------------------------------
spec_dir="$ROOT/spec"
spec_pairs_checked=0
spec_violations=0
if [ -d "$spec_dir" ]; then
    shopt -s nullglob
    for enf in "$spec_dir"/*.md; do
        case "$enf" in *.ru.md) continue ;; esac
        # basename как bash-подстановка, не форк (план 275-Ф.1, гейт-стоимость:
        # basename+head+3 grep на файл были доминирующим временем секции).
        base="${enf##*/}"; base="${base%.md}"
        ruf="$spec_dir/$base.ru.md"
        [ -f "$ruf" ] || continue  # нет ru-оригинала — это ловит check-doc-language-pairs.sh
        spec_pairs_checked=$((spec_pairs_checked + 1))
        fname="${enf##*/}"
        # Один awk вместо head+3 grep: те же три условия за один проход по
        # первым 15 строкам файла (exit после FNR>15 всё равно доходит до END).
        read -r has_header has_rev has_date < <(awk '
            FNR > 15 { exit }
            index($0, "Informative translation; the Russian text is normative.") > 0 { hh = 1 }
            $0 ~ /source_rev:[[:space:]]*[[:alnum:]]/ { hr = 1 }
            $0 ~ /source_date:[[:space:]]*[0-9]{4}-[0-9]{2}-[0-9]{2}/ { hd = 1 }
            END { printf "%d %d %d\n", hh+0, hr+0, hd+0 }
        ' "$enf")
        ok=1
        [ "$has_header" = "1" ] || { red "spec/$fname: нет шапки «Informative translation; the Russian text is normative.» в первых 15 строках"; ok=0; }
        [ "$has_rev" = "1" ] || { red "spec/$fname: нет frontmatter source_rev: в первых 15 строках"; ok=0; }
        [ "$has_date" = "1" ] || { red "spec/$fname: нет frontmatter source_date: (YYYY-MM-DD) в первых 15 строках"; ok=0; }
        [ "$ok" -eq 0 ] && spec_violations=$((spec_violations + 1))
    done
    shopt -u nullglob
fi
if [ "$spec_pairs_checked" -eq 0 ]; then
    info "doc-conventions ok (вакуумно): spec-пар с ru-стороной пока нет"
else
    [ "$spec_violations" -eq 0 ] && info "doc-conventions ok: spec_en_header — $spec_pairs_checked пар(ы), 0 нарушений"
fi

# ---------------------------------------------------------------------
# 2. docs/guide/PUBLISHED.list — парность en/ru.
# ---------------------------------------------------------------------
guide_dir="$ROOT/docs/guide"
published_list="$guide_dir/PUBLISHED.list"
guide_pair_names=""  # накапливаем имена пар (для code_block_identity/2b) как список строк
if [ -f "$published_list" ]; then
    pair_count=0
    pair_violations=0
    while IFS= read -r raw || [ -n "$raw" ]; do
        line="${raw%%#*}"
        # Обрезка пробелов bash-подстановкой вместо форка sed НА КАЖДУЮ строку
        # (план 275-Ф.1, гейт-стоимость).
        line="${line#"${line%%[![:space:]]*}"}"
        line="${line%"${line##*[![:space:]]}"}"
        [ -z "$line" ] && continue
        pair_count=$((pair_count + 1))
        en="$guide_dir/$line.md"
        ru="$guide_dir/$line.ru.md"
        pair_ok=1
        if [ ! -f "$en" ]; then
            red "guide pairing: docs/guide/$line.md отсутствует (указан в PUBLISHED.list)"
            pair_violations=$((pair_violations + 1)); pair_ok=0
        fi
        if [ ! -f "$ru" ]; then
            red "guide pairing: docs/guide/$line.ru.md отсутствует — $line опубликован без ru-пары"
            pair_violations=$((pair_violations + 1)); pair_ok=0
        fi
        [ "$pair_ok" -eq 1 ] && guide_pair_names="$guide_pair_names $line"
    done < "$published_list"
    [ "$pair_violations" -eq 0 ] && info "doc-conventions ok: guide_pairing — $pair_count имён в PUBLISHED.list, все с парой"
else
    info "doc-conventions ok (вакуумно): docs/guide/PUBLISHED.list ещё не создан (план 241-Ф.1b)"
fi

# Для code_block_identity/2b нам нужны ВСЕ фактически существующие пары,
# не только объявленные в PUBLISHED.list (правило 5 действует независимо
# от публикации — байт-идентичность обязана держаться с первого коммита
# перевода, до её включения в публичный список).
discovered_guide_pairs=""
if [ -d "$guide_dir" ]; then
    shopt -s nullglob
    for enf in "$guide_dir"/*.md; do
        # basename как bash-подстановка, не форк (план 275-Ф.1, гейт-стоимость).
        base="${enf##*/}"; base="${base%.md}"
        case "$base" in *.ru) continue ;; esac
        ruf="$guide_dir/$base.ru.md"
        [ -f "$ruf" ] || continue
        discovered_guide_pairs="$discovered_guide_pairs $base"
    done
    shopt -u nullglob
fi

# ---------------------------------------------------------------------
# 2b. same-commit pairing (best-effort, требует diff-base).
# ---------------------------------------------------------------------
if [ -n "$DIFF_BASE" ] && git -C "$ROOT" cat-file -e "$DIFF_BASE^{commit}" >/dev/null 2>&1; then
    changed="$(git -C "$ROOT" diff --name-only "$DIFF_BASE" -- docs/guide spec 2>/dev/null)"
    same_commit_violations=0
    check_pair_same_commit() {  # en_rel ru_rel label
        local en_rel="$1" ru_rel="$2" label="$3"
        local en_changed=0 ru_changed=0
        printf '%s\n' "$changed" | grep -qxF "$en_rel" && en_changed=1
        printf '%s\n' "$changed" | grep -qxF "$ru_rel" && ru_changed=1
        [ "$en_changed" -eq "$ru_changed" ] && return 0  # оба или ни одного — ок
        local lone
        [ "$en_changed" -eq 1 ] && lone="$en_rel" || lone="$ru_rel"
        local diff_lines
        diff_lines="$(git -C "$ROOT" diff "$DIFF_BASE" -- "$lone" 2>/dev/null | grep -E '^[+-][^+-]')"
        # Допуск: диапазон правит ТОЛЬКО source_rev:/source_date: — не нарушение.
        if [ -n "$diff_lines" ] && ! printf '%s\n' "$diff_lines" | grep -qvE '^[+-][[:space:]]*(source_rev|source_date):'; then
            return 0
        fi
        info "doc-conventions ПРЕДУПРЕЖДЕНИЕ (same-commit pairing): $lone изменён без пары ($label) в диапазоне $DIFF_BASE..HEAD — переводческие волны правят одну сторону законно; проверка наблюдательная (№322), гейт не роняет"
        same_commit_violations=$((same_commit_violations + 1))
    }
    for name in $discovered_guide_pairs; do
        check_pair_same_commit "docs/guide/$name.md" "docs/guide/$name.ru.md" "$name"
    done
    if [ -d "$spec_dir" ]; then
        shopt -s nullglob
        for enf in "$spec_dir"/*.md; do
            case "$enf" in *.ru.md) continue ;; esac
            base="${enf##*/}"; base="${base%.md}"
            [ -f "$spec_dir/$base.md" ] || continue
            check_pair_same_commit "spec/$base.ru.md" "spec/$base.md" "$base"
        done
        shopt -u nullglob
    fi
    [ "$same_commit_violations" -eq 0 ] && info "doc-conventions ok: guide_same_commit — диапазон $DIFF_BASE..HEAD, 0 однобоких правок"
else
    info "doc-conventions: guide_same_commit пропущен (нет diff-base — передай 2-м аргументом или DOC_GUARD_DIFF_BASE; CI передаёт явно)"
fi

# ---------------------------------------------------------------------
# Ratchet-хелпер (образец: arch-ratchet.sh). key=значение построчно в
# baseline; текущее значение читается через переменную m_<key>.
# ---------------------------------------------------------------------
ratchet_check() {  # key current_value description
    local key="$1" cur="$2" desc="$3" base
    # Только СТРОКИ-ЗНАЧЕНИЯ: ключ=целое до конца строки. Комментарий,
    # потерявший ведущую '#', больше не может быть принят за значение
    # (№290: именно так страж молча уходил в ветку «ok» на любом числе).
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
        info "doc-conventions ok: $key=$cur <= baseline=$base ($desc)"
    fi
}

# ---------------------------------------------------------------------
# 3. plan_status (ratchet): docs/plans/NNN-*.md без **Статус:**.
# ---------------------------------------------------------------------
plans_dir="$ROOT/docs/plans"
plan_missing_status=0
if [ -d "$plans_dir" ]; then
    # `sed 's#.*/##'` вместо `xargs -n1 basename` (план 275-Ф.1, гейт-стоимость):
    # 466 файлов без **Статус:** на момент замера — 466 форков внешнего
    # basename на КАЖДОЕ имя, доминирующие расходы стража (~30-40с из ~42с).
    # sed делает то же самое ОДНИМ процессом на весь список.
    plan_missing_status=$(grep -L -E '^\*\*Статус:\*\*' "$plans_dir"/*.md 2>/dev/null \
        | sed 's#.*/##' \
        | grep -vE '^(README\.md|STATUS\.md)$' \
        | grep -vE '(-notes|-progress|-execution-plan|-session[^.]*|-history)\.md$' \
        | grep -cE '^[0-9]')
fi
ratchet_check plan_missing_status "$plan_missing_status" "docs/plans/NNN-*.md без строки **Статус:**"

# ---------------------------------------------------------------------
# 4. dev_links (ratchet): ссылки на docs/dev/ из docs/guide/** и spec/**.
# ---------------------------------------------------------------------
dev_link_pattern='(\.\./dev/|\.\./docs/dev/|docs/dev/)'
# №315/№318: docs/plans/ так же не публикуется, как docs/dev/ — считаем ТОЛЬКО
# кликабельные ссылки (markdown-цель), голые упоминания «Plan 210» в прозе
# нарушением НЕ считаются (та же граница, что у проверки смешения языков:
# метка-адрес — не дыра). Отдельный храповик, цель — «видно и не растёт».
plans_link_pattern='\]\([^)]*(\.\./plans/|docs/plans/)[^)]*\)'
dev_links=0
scan_dirs=""
[ -d "$guide_dir" ] && scan_dirs="$scan_dirs $guide_dir"
[ -d "$spec_dir" ] && scan_dirs="$scan_dirs $spec_dir"
if [ -n "$scan_dirs" ]; then
    # Периметр СУЖЕН (решение владельца 2026-08-03): spec/decisions/** НЕ
    # считается. ВАЖНО (№331): не потому, что «не публикуется» — decisions
    # как раз публикуются (RU-only, doc-conventions #zones; синк сайта их
    # пишет наравне с guide). Исключены ПО ОБЪЁМУ И ПРИРОДЕ: ~64k строк
    # рабочего норматива с плотной перекрёстной адресацией, где ссылки на
    # процесс уместны; возврат их в счёт дал бы скачок базы (в первом замере
    # dev_links это 56 из 113) без пользы для читателя гайдов.
    dev_links=$(grep -rhoE "$dev_link_pattern" $scan_dirs 2>/dev/null         --exclude-dir=decisions | wc -l)
fi
ratchet_check dev_links "$dev_links" "ссылки на docs/dev/ из docs/guide/** + spec/*.md без decisions/ (никогда не публикуется — #publishing)"

plans_links=0
if [ -n "$scan_dirs" ]; then
    plans_links=$(grep -rhoE "$plans_link_pattern" $scan_dirs 2>/dev/null         --exclude-dir=decisions | wc -l)
fi
ratchet_check plans_links "$plans_links" "кликабельные ссылки в docs/plans/ из docs/guide/** + spec/*.md (зона не публикуется — #publishing)"

# ---------------------------------------------------------------------
# 5. code_block_identity (ratchet): байт-идентичность ```-фенсов пар.
# ---------------------------------------------------------------------
# fences_match: ОДИН awk на пару вместо ДВУХ (extract_code_fences на каждую
# сторону + сравнение строк в bash) — план 275-Ф.1, гейт-стоимость: при ~35
# парах это было 70 форков awk, доминирующие расходы секции (~5с из ~21с).
# awk сам читает ОБА файла (два позиционных аргумента, FNR==1 отличает
# файл от файла) и сравнивает извлечённые фенсы построчно, не выходя из
# процесса — исход тот же самый (байт-в-байт список строк внутри нечётных
# ```-блоков), только за один форк, а не за два плюс сравнение в bash.
fences_match() {  # file1 file2 -> exit 0 если фенсы совпадают, 1 если нет
    awk '
        FNR == 1 { fidx++ }
        /^```/ { c[fidx]++; next }
        { if (c[fidx] % 2 == 1) buf[fidx] = buf[fidx] $0 "\n" }
        END { exit (buf[1] == buf[2]) ? 0 : 1 }
    ' "$1" "$2" 2>/dev/null
}
code_block_mismatch_pairs=0
mismatch_names=""
for name in $discovered_guide_pairs; do
    if ! fences_match "$guide_dir/$name.md" "$guide_dir/$name.ru.md"; then
        code_block_mismatch_pairs=$((code_block_mismatch_pairs + 1))
        mismatch_names="$mismatch_names docs/guide/$name"
    fi
done
if [ -d "$spec_dir" ]; then
    shopt -s nullglob
    for enf in "$spec_dir"/*.md; do
        case "$enf" in *.ru.md) continue ;; esac
        base="${enf##*/}"; base="${base%.md}"
        ruf="$spec_dir/$base.ru.md"
        [ -f "$ruf" ] || continue
        if ! fences_match "$enf" "$ruf"; then
            code_block_mismatch_pairs=$((code_block_mismatch_pairs + 1))
            mismatch_names="$mismatch_names spec/$base"
        fi
    done
    shopt -u nullglob
fi
[ -n "$mismatch_names" ] && info "doc-conventions: код-блоки расходятся у пар:$mismatch_names (долг, см. baseline)"
ratchet_check code_block_mismatch_pairs "$code_block_mismatch_pairs" "пары X.md/X.ru.md с несовпадающими code-fence блоками"

# ---------------------------------------------------------------------
# ---------------------------------------------------------------------
# 5б. manifest_genre: страницы «внутреннего» жанра в манифесте публикации.
#     site-conventions #page-genre (2026-08-05): roadmap/changelog/планы/wip/
#     design-notes НЕ публикуются по умолчанию — план развития на сайте
#     читается как обещание. Прецедент: roadmap полариса попал в публикацию
#     механически. Проверка КРАСНАЯ: следующий такой файл не должен уехать
#     на сайт незаметно; осознанное исключение = закомментированная строка.
# ---------------------------------------------------------------------
for mf in "$ROOT/docs/guide/PUBLISHED.list" "$ROOT/docs/PUBLISHED.list"; do
    [ -f "$mf" ] || continue
    genre_hits=$(grep -vE '^[[:space:]]*(#|$)' "$mf"         | grep -ciE '^(roadmap|changelog|todo|wip([-_].*)?|plans?([-_].*)?|design-notes?)$')
    if [ "${genre_hits:-0}" -gt 0 ]; then
        red "manifest_genre: $mf публикует страницу внутреннего жанра (roadmap/changelog/plans/wip) — site-conventions #page-genre; исключи строку или закомментируй с причиной"
    fi
done

# 6. readme_pair: README пакета/модуля — ВСЕГДА пара en+ru.
#    Решение владельца 2026-08-03: «ридми для пакетов, модулей всегда
#    на анг + рус». Проверка безусловная (не храповик): есть README.md —
#    обязан быть README.ru.md, и наоборот. Репа без README вовсе —
#    вакуумно-зелёная (проверять нечего). Работает и для nova, и для
#    пакетных реп (§2b): скрипт принимает корень репы аргументом.
# ---------------------------------------------------------------------
readme_en="$ROOT/README.md"
readme_ru="$ROOT/README.ru.md"
if [ -f "$readme_en" ] || [ -f "$readme_ru" ]; then
    if [ ! -f "$readme_ru" ]; then
        red "readme_pair: есть README.md, нет README.ru.md (пара обязательна — решение владельца 2026-08-03)"
    elif [ ! -f "$readme_en" ]; then
        red "readme_pair: есть README.ru.md, нет README.md (пара обязательна — решение владельца 2026-08-03)"
    else
        # код-блоки README-пары — по общему правилу байт-в-байт
        ra="$(awk '/^```/{f=!f; next} f' "$readme_en")"
        rb="$(awk '/^```/{f=!f; next} f' "$readme_ru")"
        if [ "$ra" = "$rb" ]; then
            info "doc-conventions ok: readme_pair — README.md + README.ru.md, код-блоки идентичны"
        else
            red "readme_pair: код-блоки README.md и README.ru.md расходятся (#translation-drift)"
        fi
    fi
else
    info "doc-conventions ok (вакуумно): README в корне нет — проверять нечего"
fi

# ---------------------------------------------------------------------
# 7. mixed_language (ratchet): кириллица в файлах, которые ОБЯЗАНЫ быть
#    английскими (X.md без `.ru.` — и в docs/guide, и в spec, README.md).
#    Грубая эвристика по совету консультанта: считаем файлы, где ВНЕ
#    ```-блоков больше 1 строки с кириллицей (одна допустима — строка
#    переключателя языка со словом «Русский»). Ловит именно смешение
#    языков внутри файла (прецедент: 60 строк русского раздела про Z3
#    в титульном английском README), в отличие от словаря калек,
#    который тонет в именах кода.
# ---------------------------------------------------------------------
mixed_language_files=0
mixed_names=""
check_mixed() {  # file
    [ -f "$1" ] || return 0
    # Считаем только СОДЕРЖАТЕЛЬНУЮ кириллицу: пропускаем строку переключателя
    # языка и строки со ссылками (там кириллица — это якоря русских планов и
    # D-блоков, они законны, см. план 247).
    # Пропускаем законные адреса русских документов: строку переключателя,
    # строки со ссылками, пометки фаз планов («Ф.6а») и цитаты-якоря вида
    # «D412-амендмент» — по ним читатель ищет место в русском первоисточнике.
    # Кириллица законна ТОЛЬКО в адресах русских документов: цели ссылок
    # (URL/якорь), пометки фаз планов («Ф.6а»), цитаты-якоря
    # («D412-амендмент»), параграфы («§2л») и слово «Русский» в
    # переключателе языка. Всё это вырезается из строки, а ОСТАТОК строки
    # обязан быть без кириллицы — иначе это недопереведённая проза.
    # (Урок №314: пропускать строку со ссылкой ЦЕЛИКОМ нельзя — именно там
    # пряталось «Read [X](...) для decision trees».)
    n=$(awk '
        /^```/{f=!f; next}
        f {next}
        {
            line=$0
            gsub(/\]\([^)]*\)/, "]", line)          # цель ссылки — адрес
            gsub(/`[^`]*`/, "", line)               # inline-код
            gsub(/Ф\.[0-9]+[^ ,)\]]*/, "", line)    # пометки фаз планов
            gsub(/[A-Za-z0-9_]+-амендмент/, "", line)
            gsub(/§[0-9]+[^ ,)\]]*/, "", line)
            gsub(/Русский/, "", line)               # переключатель языка
            if (line ~ /[\xd0-\xd1]/) c++
        }
        END{print c+0}' "$1")
    if [ "$n" -gt 1 ]; then
        mixed_language_files=$((mixed_language_files + 1))
        mixed_names="$mixed_names ${1##*/}($n)"
    fi
}
# ---------------------------------------------------------------------
# 7б. code_comment_ru (ratchet): русские комментарии ВНУТРИ ```-блоков
#     английских файлов. Класс, который правило байт-идентичности сторон
#     пары прятало от проверки 7 (она смотрит только вне блоков), а
#     англоязычный читатель видит их на странице. Лечится синхронной
#     правкой ОБЕИХ сторон пары (решение владельца 2026-08-04 про
#     ASCII-диаграммы распространено на комментарии примеров).
# ---------------------------------------------------------------------
code_comment_ru_files=0
ccru_names=""
# README и guide-файлы проверяются ОБОИМИ правилами (7 и 7б) — раньше это
# было ДВА форка awk на файл (check_mixed + check_ccru), один проход по
# файлу каждый. План 275-Ф.1 (гейт-стоимость): при ~30 guide-файлах + README
# это было ~62 форка awk, вторая по весу статья расходов стража (~3.6с из
# ~21с). Один awk считает ОБА счётчика за ОДИН проход по строкам — состояние
# `f` (внутри/вне фенса) то же самое, что было у двух раздельных скриптов,
# просто ветвление на f/!f сведено в один if/else вместо двух независимых
# фильтров `f{next}` / `!f{next}` в двух процессах. spec-файлы (правило 7б
# на них не действует) по-прежнему идут через check_mixed в одиночку.
check_mixed_and_ccru() {  # file
    [ -f "$1" ] || return 0
    read -r n_out n_in < <(awk '
        /^```/ { f = !f; next }
        {
            if (!f) {
                line = $0
                gsub(/\]\([^)]*\)/, "]", line)
                gsub(/`[^`]*`/, "", line)
                gsub(/Ф\.[0-9]+[^ ,)\]]*/, "", line)
                gsub(/[A-Za-z0-9_]+-амендмент/, "", line)
                gsub(/§[0-9]+[^ ,)\]]*/, "", line)
                gsub(/Русский/, "", line)
                if (line ~ /[\xd0-\xd1]/) c_out++
            } else {
                line2 = $0
                gsub(/Ф\.[0-9]+[^ ,)\]]*/, "", line2)
                if (line2 ~ /[\xd0-\xd1]/) c_in++
            }
        }
        END { printf "%d %d\n", c_out+0, c_in+0 }
    ' "$1")
    if [ "$n_out" -gt 1 ]; then
        mixed_language_files=$((mixed_language_files + 1))
        mixed_names="$mixed_names ${1##*/}($n_out)"
    fi
    if [ "$n_in" -gt 0 ]; then
        code_comment_ru_files=$((code_comment_ru_files + 1))
        ccru_names="$ccru_names ${1##*/}($n_in)"
    fi
}
check_mixed_and_ccru "$ROOT/README.md"
if [ -d "$guide_dir" ]; then
    for f in "$guide_dir"/*.md; do
        case "$f" in *.ru.md) continue ;; esac
        check_mixed_and_ccru "$f"
    done
fi
if [ -d "$spec_dir" ]; then
    for f in "$spec_dir"/*.md; do
        case "$f" in *.ru.md) continue ;; esac
        check_mixed "$f"
    done
fi
[ -n "$ccru_names" ] && info "doc-conventions: русские комментарии в примерах английских страниц:$ccru_names"
if [ "$code_comment_ru_files" -gt 0 ]; then
    red "code_comment_ru: $code_comment_ru_files английских файлов содержат русские комментарии внутри примеров (правится СИНХРОННО в обеих сторонах пары)"
else
    info "doc-conventions ok: code_comment_ru — русских комментариев в примерах английских страниц нет"
fi

[ -n "$mixed_names" ] && info "doc-conventions: кириллица в английских файлах:$mixed_names (строк вне код-блоков)"
# ПРЯМАЯ проверка, не храповик (совет ревью): долг погашен до нуля, а
# baseline=0 означает ровно «будь нулём» — чтение базы лишь маскировало бы
# строгий инвариант и оставляло щель вписать туда «3 с запасом».
if [ "$mixed_language_files" -gt 0 ]; then
    red "mixed_language: $mixed_language_files английских файлов содержат русскую прозу (допустимы только адреса: цели ссылок, «Ф.N», якоря)"
else
    info "doc-conventions ok: mixed_language — русской прозы в английских файлах нет"
fi

# ---------------------------------------------------------------------
# 8. translation_drift (ПРЕДУПРЕЖДЕНИЕ НАВСЕГДА, не храповик — совет ревью).
#    Для каждого английского spec/X.md сравниваем source_rev с последним коммитом,
#    тронувшим источник spec/X.md, и печатаем ГРАДУИРОВАННУЮ величину:
#    сколько СОДЕРЖАТЕЛЬНЫХ строк набежало (правки одного frontmatter не
#    считаются). Порога нет намеренно: срочность оценивает человек.
#    Храповик здесь структурно неверен — отставание перевода это не разово
#    гасимый долг, а постоянная задержка процесса: норматив ведёт одна
#    сессия, перевод догоняет другая.
#    УСЛОВИЕ УЖЕСТОЧЕНИЯ (фиксируется заранее, чтобы «временное» не осталось
#    навсегда): как только перевод и норматив ведутся одной сессией ИЛИ
#    появляется автоматический догон, проверка становится красной при
#    дрейфе > 50 содержательных строк.
# ---------------------------------------------------------------------
if [ -d "$spec_dir" ] && command -v git >/dev/null 2>&1 &&
   git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    drift_lines_total=0
    drift_names=""
    for enf in "$spec_dir"/*.md; do
        [ -f "$enf" ] || continue
        case "$enf" in *.ru.md) continue ;; esac
        base_name="$(basename "$enf" .md)"
        ruf="spec/$base_name.ru.md"
        [ -f "$ROOT/$ruf" ] || continue
        rev=$(awk -F': *' '/^source_rev:/{gsub(/[" ]/,"",$2); print $2; exit}' "$enf")
        [ -n "$rev" ] || continue
        git -C "$ROOT" cat-file -e "$rev^{commit}" 2>/dev/null || continue
        # содержательные строки: без правок только-frontmatter
        n=$(git -C "$ROOT" diff --unified=0 "$rev..HEAD" -- "$ruf" 2>/dev/null             | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)'             | grep -vE '^[+-](source_rev|source_date):' | wc -l)
        if [ "${n:-0}" -gt 0 ]; then
            drift_lines_total=$((drift_lines_total + n))
            drift_names="$drift_names $base_name(+$n)"
        fi
    done
    if [ -n "$drift_names" ]; then
        info "doc-conventions ИНФОРМАЦИОННО (translation_drift):$drift_names содержательных строк набежало с момента перевода. ДЕЙСТВИЕ НЕ ОТ ТЕБЯ, если ты ведёшь норматив: копится в бэклог волны перевода (план 241 Ф.3)"
    else
        info "doc-conventions ok: translation_drift — переводы спеки соответствуют своим ревизиям"
    fi
fi

# Своя строка на выходе — требование обёртки guard() в гейте: ноль без строки
# значит «не упал», а не «проверил» (реестр 221.1 №645).
[ "$fail" -eq 0 ] && echo "check-doc-conventions ok: шапки, язык и парность страниц в порядке"
exit $fail
