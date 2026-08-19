#!/bin/sh
# scripts/guards/check-novac-module-donor.sh — у каждого модуля novac в
# заголовке названа форма-донор указателем, либо честно сказано «донора нет»
# (конвенция П27, вторая половина; владелец 2026-08-16).
#
# ЗАЧЕМ ОТДЕЛЬНО ОТ КОММИТ-СТРАЖА. Сообщение коммита — след во времени: оно
# говорит, откуда взято решение В ТОТ ДЕНЬ. Заголовок модуля — постоянная
# правда о его форме: читающий код через год не пойдёт по истории коммитов.
# И у заголовка есть то, чего у коммита нет: граница «взято / НЕ взято» —
# донора никогда не берут целиком (у rustc интернер, но не арены).
#
# ТРИ ЧАСТИ, НЕ ОДНА (уточнение владельца 2026-08-16): указатель без места в
# общей картине — просто ссылка. Заголовок обязан ответить на три вопроса:
# у кого взято (Donor), зачем это в общем плане (Role), кто потребитель дальше
# и когда (Used by). Решение, потребитель которого не назван, потеряет его при
# первой переделке.
#
# ПРОВЕРЯЕТ по novac/src/**/*.nv (тесты *_test.nv исключены), в ПЕРВЫХ 40
# строках файла (заголовочный комментарий):
#   * есть строка `// Donor:` (либо `/// Donor:`, либо `// Donor —`);
#   * есть строка `// Role:` — минимум четыре слова (место в карте / класс задачи);
#   * есть строка `// Used by:` — минимум три слова (потребитель и этап);
#   * есть строка `// Guarded by:` — кто АВТОМАТИЧЕСКИ проверяет правильное
#     использование дальше: каждый названный `check-*.sh`/`check-*.py` обязан существовать
#     файлом в scripts/guards (несуществующий — красный: механизм не назван,
#     а выдуман); честные формы без файла — `compiler — ...` и
#     `acceptance — ...` (норма 254);
#   * после неё — минимум два слова (донор + сущность), ЛИБО `none` и после
#     тире минимум пять слов причины;
#   * голое имя языка (`Donor: rustc`) — красный: это утверждение, не указатель.
# НЕ ПРОВЕРЯЕТ: правдивость указателя (приёмка); наличие границы «взято/не
#   взято» словами (проза; страж заставляет назвать источник, а не пересказать
#   его); файлы-спутники модуля без собственного решения (папка = модуль,
#   но каждый файл несёт форму — если он второй файл модуля и решения не
#   вводит, законно `Donor: see <главный файл модуля>` — два слова есть).
#
# $1 — корень репозитория; $2 — override сканируемой директории (самотест).
# Проверялся: Windows (Git Bash), 2026-08-16.
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
SRC="${2:-$ROOT/novac/src}"
NAME=check-novac-module-donor

if [ ! -d "$SRC" ]; then
    echo "$NAME ok: судить нечего (нет $SRC)"
    exit 0
fi

# ОДИН проход awk по всем файлам разом (2026-08-18). Прежняя редакция поднимала
# около дюжины процессов на каждый файл и ещё по одному на имя стража в строке
# `Guarded by`; на 27 файлах это 53.9 секунды стены, из которых работой не было
# ничего. Правила ниже — те же, слово в слово; доказательство — самотест и
# сравнение вывода на живом дереве.
GUARDDIR="$ROOT/scripts/guards"
# Имена стражей собираются ОДИН раз: `test -f` внутри awk поднимал процесс
# на каждое имя в строке `Guarded by`, а их десятки (2026-08-19).
GUARDLIST=$(ls "$GUARDDIR" 2>/dev/null | tr "\n" " ")
BAD=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | sort | xargs awk -v SRC="$SRC" -v GUARDLIST="$GUARDLIST" '
    BEGIN {
        _n = split(GUARDLIST, _g, / /)
        for (_i = 1; _i <= _n; _i++) if (_g[_i] != "") GUARDS[_g[_i]] = 1
    }
    function words(s,   n, a) { gsub(/^[ \t]+|[ \t]+$/, "", s); if (s == "") return 0; n = split(s, a, /[ \t]+/); return n }
    function say(msg) { printf "  %s: %s\n", rel, msg }

    FNR == 1 {
        if (NR > 1) judge()
        rel = FILENAME; sub("^" SRC "/", "", rel)
        donor = ""; role = ""; used = ""; guarded = ""; dblock = ""
        seen_donor = 0; in_d = 0
    }
    FNR > 40 { next }
    {
        line = $0; sub(/\r$/, "", line)
        if (line ~ /^\/\/\/? *Donor *[:—-]/) {
            if (!seen_donor) { donor = line; sub(/^\/\/\/? *Donor *[:—-] */, "", donor); seen_donor = 1 }
            in_d = 1
        } else if (line ~ /^\/\/\/? *Role *[:—-]/) {
            in_d = 0
            if (role == "") { role = line; sub(/^\/\/\/? *Role *[:—-] */, "", role) }
        } else if (line ~ /^\/\/\/? *Used by *[:—-]/) {
            if (used == "") { used = line; sub(/^\/\/\/? *Used by *[:—-] */, "", used) }
        } else if (line ~ /^\/\/\/? *Guarded by *[:—-]/) {
            if (guarded == "") { guarded = line; sub(/^\/\/\/? *Guarded by *[:—-] */, "", guarded) }
            else guarded = guarded " " line
        } else if (guarded != "" && line ~ /^\/\/\/? +/) {
            # продолжение строки Guarded by переносом
            cont = line; sub(/^\/\/\/? */, "", cont)
            if (cont ~ /check-[a-z0-9-]+\.(sh|py)/) guarded = guarded " " cont
        }
        if (in_d) dblock = dblock " " line
    }
    END { judge() }

    function judge(   body, reason, nw, g, i, n, arr, gname) {
        if (rel == "") return
        if (!seen_donor) { say("нет строки \x27// Donor:\x27 в заголовке (первые 40 строк)"); rel = ""; return }
        body = donor
        if (tolower(body) ~ /^none[ \t]*(—|-|--)/) {
            reason = body; sub(/^[Nn][Oo][Nn][Ee][ \t]*(—|-|--)[ \t]*/, "", reason)
            if (words(reason) < 5) say("\x27Donor: none\x27 без причины (нужно минимум пять слов)")
            body = "none reason ok"
        }
        if (words(body) < 2) say("\x27Donor:\x27 без сущности — одно имя не указатель: «" donor "»")

        if (tolower(dblock) ~ /(^|[^a-z])swift([^a-z]|$)/ || dblock ~ /C#/ || dblock ~ /[.]NET/) {
            if (tolower(dblock) !~ /not taken|anti-example|not a donor|not from/)
                say("Donor называет Swift/C# без формы отказа «NOT taken ...» — антипример выдан за донора")
        }
        if (tolower(dblock) ~ /(^|[^a-z])zig([^a-z]|$)/ && dblock !~ /InternPool|Sema|StaticStringMap|OptionalIndex|std[.]/)
            say("Donor называет Zig без его сущности (InternPool/Sema/...) — точечный донор без места")
        if (tolower(dblock) ~ /oracle|orakul|nova-cli|compiler-codegen|emit_c\.rs/)
            say("\x27Donor:\x27 называет ОРАКУЛ (нынешний компилятор) донором — запрещено (П25)")

        if (words(role) < 4) say("нет строки \x27// Role:\x27 с местом в общей картине (минимум четыре слова)")
        if (words(used) < 3) say("нет строки \x27// Used by:\x27 — кто потребитель дальше и когда")
        if (words(guarded) < 1) {
            say("нет строки \x27// Guarded by:\x27 — кто автоматически проверяет правило")
        } else {
            g = guarded
            n = 0
            while (match(g, /check-[a-z0-9-]+\.(sh|py)/)) {
                gname = substr(g, RSTART, RLENGTH)
                if (!(gname in GUARDS))
                    say("\x27Guarded by\x27 называет " gname ", а такого стража нет в scripts/guards — механизм выдуман")
                g = substr(g, RSTART + RLENGTH)
                n++
            }
            if (n == 0 && guarded !~ /^compiler|^acceptance|nova test|nova lint|fuzz/)
                say("\x27Guarded by\x27 не называет ни стража, ни теста, ни честного compiler/acceptance")
        }
        rel = ""
    }
')

if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — модуль novac без донора-указателя в заголовке (конвенция П27):" >&2
    printf '%s\n' "$BAD" >&2
    echo "  В первых 40 строках три строки: '// Donor: <кто> <сущность> — взято/не взято'," >&2
    echo "  '// Role: <место в карте слоёв, класс задачи>', '// Used by: <кто читает, на каком этапе>'," >&2
    echo "  '// Guarded by: <check-*.sh или check-*.py, тесты — кто автоматически ловит неверное использование>'." >&2
    echo "  Донора нет — честно: '// Donor: none — <причина>'; Role и Used by нужны всё равно." >&2
    exit 1
fi

N=$(find "$SRC" -type f -name '*.nv' ! -name '*_test.nv' | wc -l | tr -d '[:space:]')
echo "$NAME ok: модулей novac: $N, у всех донор назван указателем или честно отсутствует"
exit 0
