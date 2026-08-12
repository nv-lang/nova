#!/usr/bin/env bash
# scripts/guards/check-staged-secrets.sh
# Секрет вреден самим фактом попадания в историю.
#
# ДОМ И ОСНОВАНИЕ: реестр 221.1 №TBD-secrets; перенесено 2026-08-12 из
# другого проекта владельца по его указанию.
#
# ЗАЧЕМ ИМЕННО НАМ. У нас секреты ходят рядом с кодом: в URL зеркала
# `sourcecraft` лежит токен — настолько, что в конвенции стоит правило
# «никогда не звать `git remote -v`», и держится оно ПАМЯТЬЮ интегратора.
# Правило без машины мы за один день признали проигрышным дважды (№596, №612).
# Один невнимательный `git add` — и токен в истории навсегда: переписать её на
# трёх зеркалах дороже, чем завести эту проверку.
#
# ДВА РЕЖИМА, и они отвечают на разные вопросы.
#   (по умолчанию) STAGED — что уходит в коммит ПРЯМО СЕЙЧАС. Смотрит только
#       ДОБАВЛЯЕМЫЕ строки: вопрос «сколько их всего в репозитории» здесь
#       бессмысленный. Место вызова — `scripts/githooks/pre-commit`.
#   --tree — что лежит в дереве. Только правила ВЫСОКОЙ уверенности (ключ,
#       токен, пароль внутри адреса): эвристику «слово password рядом со
#       значением» на всё дерево пускать нельзя — у нас доки, в которых про
#       пароли ПИШУТ, и страж утонул бы в собственных объяснениях.
#
# ЧЕГО НЕ ЛОВИТ (сказано честно): секрет, не похожий на секрет, — случайная
# строка без префикса и без ключевого слова. Страж ловит ФОРМЫ, которые мы
# знаем; он сужает окно ошибки, а не закрывает его.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-staged-secrets.sh            # staged
#   bash scripts/guards/check-staged-secrets.sh --tree [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-staged-secrets.sh

set -u
export LC_ALL=C

MODE="staged"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
while [ $# -gt 0 ]; do
    case "$1" in
        --tree) MODE="tree"; shift ;;
        -*) echo "check-staged-secrets: неизвестный флаг '$1'" >&2; exit 1 ;;
        *)  ROOT="$1"; shift ;;
    esac
done

BAD=0
report() {
    echo "check-staged-secrets: НАРУШЕНИЕ — $1" >&2
    printf '%s\n' "$2" | sed 's/^/    /' >&2
    BAD=1
}

# Самотест исключён из периметра намеренно: его фикстуры ПО ПРИРОДЕ содержат
# подложенные ключи и токены. Детектор не должен ловить сам себя — иначе он
# либо краснеет на себе, либо (хуже) его чинят ослаблением правила.
EXCL_SELFTEST=':(exclude)scripts/guards/selftest/'
EXCL_SELF=':(exclude)scripts/guards/check-staged-secrets.sh'

if [ "$MODE" = "staged" ]; then
    DIFF=$(git -C "$ROOT" diff --cached --unified=0 -- . "$EXCL_SELFTEST" "$EXCL_SELF" 2>/dev/null || true)
    [ -n "$DIFF" ] || { echo "check-staged-secrets ok: staged пуст"; exit 0; }
    SCAN=$(printf '%s\n' "$DIFF" | grep '^+' | grep -v '^+++' || true)
else
    # Именной список законных мест. Каждая строка обязана нести причину:
    # путь без причины — это пропуск, выписанный молча, а таких у нас не бывает
    # (тот же приём, что в `std-test-fail.baseline` и `doc-pair-missing.baseline`).
    ALLOW_FILE="${NOVA_SECRETS_ALLOWLIST:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/secrets-allowlist.baseline}"
    ALLOW_RE=""
    if [ -f "$ALLOW_FILE" ]; then
        while IFS= read -r line; do
            case "$line" in ''|'#'*) continue ;; esac
            path=$(printf '%s' "$line" | awk '{print $1}')
            case "$line" in
                *"#"*) : ;;
                *) report "строка списка исключений без причины: $path" "(причина обязательна)" ;;
            esac
            ALLOW_RE="${ALLOW_RE}${ALLOW_RE:+|}^$(printf '%s' "$path" | sed 's/[.[\*^$]/\\&/g')"
        done < "$ALLOW_FILE"
    fi

    FILES=$(git -C "$ROOT" ls-files -- . "$EXCL_SELFTEST" "$EXCL_SELF" 2>/dev/null \
            | grep -vE '^(compiler-codegen/(nova_rt/(libuv|gc|libatomic_ops)|vcpkg_installed))/' || true)
    if [ -n "$ALLOW_RE" ]; then
        FILES=$(printf '%s\n' "$FILES" | grep -vE "$ALLOW_RE" || true)
    fi
    [ -n "$FILES" ] || { echo "check-staged-secrets ok: git не отдал списка файлов"; exit 0; }
    SCAN=$(cd "$ROOT" && printf '%s\n' "$FILES" | tr '\n' '\0' \
           | xargs -0 -r grep -nHE 'BEGIN (RSA |OPENSSH |EC |DSA )?PRIVATE KEY|glpat-[A-Za-z0-9_-]{10,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|[a-z]+://[^/ ]+:[^/ @]+@' 2>/dev/null || true)
fi

# 1. Приватный ключ. Содержимое НЕ печатаем — печать утечки это тоже утечка.
M=$(printf '%s\n' "$SCAN" | grep -E 'BEGIN (RSA |OPENSSH |EC |DSA )?PRIVATE KEY' || true)
[ -n "$M" ] && report "приватный ключ" "(содержимое не печатается)"

# 2. Токены известных форм.
M=$(printf '%s\n' "$SCAN" | grep -E 'glpat-[A-Za-z0-9_-]{10,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}' || true)
[ -n "$M" ] && report "токен известной формы (glpat-/ghp_/github_pat_)" "(содержимое не печатается)"

# 3. Учётные данные внутри адреса: <схема>://<логин>:<пароль>@<хост>.
#    Именно так у нас выглядит remote sourcecraft — самый вероятный носитель.
M=$(printf '%s\n' "$SCAN" | grep -E '[a-z]+://[^/ ]+:[^/ @]+@' || true)
[ -n "$M" ] && report "логин с паролем внутри URL (так выглядит наш remote sourcecraft)" "(содержимое не печатается)"

# 4. Непустое значение у ключевого слова — только в staged-режиме (см. шапку).
if [ "$MODE" = "staged" ]; then
    M=$(printf '%s\n' "$SCAN" \
        | grep -iE "(password|passwd|secret|token|api_key)['\"]?[[:space:]]*[:=][[:space:]]*['\"][^'\"]{6,}['\"]" \
        | grep -viE "['\"](password|passwd|secret|token|api_key|changeme|xxx+|placeholder|example|<[^>]*>)['\"]" || true)
    [ -n "$M" ] && report "похоже на пароль/токен в присваивании — проверь глазами" "$(printf '%s\n' "$M" | cut -c1-48 | sed 's/$/…/')"
fi

if [ "$BAD" -eq 1 ]; then
    echo "" >&2
    echo "    Секрет вреден самим фактом попадания: историю на трёх зеркалах" >&2
    echo "    переписывать дороже, чем остановиться здесь." >&2
    echo "    Если срабатывание ложное — обходи ОСОЗНАННО: git commit --no-verify," >&2
    echo "    и скажи об этом в сообщении коммита." >&2
    echo "check-staged-secrets: FAIL" >&2
    exit 1
fi

echo "check-staged-secrets ok: секретов в $([ "$MODE" = tree ] && echo дереве || echo staged) не найдено"
exit 0
