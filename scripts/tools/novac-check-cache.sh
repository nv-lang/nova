#!/bin/sh
# scripts/tools/novac-check-cache.sh — ОДИН прогон `novac check` по корпусу
# novac/fixtures/**/*.nv в каталог-кэш на прогон гейта (реестр 221.1 №1717).
#
# Зачем: пять стражей корпуса звали `novac check` по одним и тем же фикстурам
# (~1200 запусков вместо ~390, замер CI 2026-10-05, run 37353522969), и трое из
# них снимались пределом 600с. Стражи читают итог дверью novac_check
# (scripts/guards/lib/novac.sh); записи нет — зовут novac сами, так что
# оборванный прогон этого инструмента не делает ни одного стража зелёным
# понарошку, он только не экономит.
#
# Запись на фикстуру: <ключ>.path (путь), .out, .err и последним .rc — читатель
# верит записи только при числовом .rc и совпавшем пути.
#
# КАК ИДЁТ ПРОГОН (реестр 221.1 №2020, 2026-11-22). Прежде — по процессу на
# фикстуру: 625 процессов, стена 934с на четырёх ядрах CI, и почти вся она —
# СТАРТ процесса (чтение и разбор std, снятие interop с оболочки), а не суд
# фикстуры. `novac check` читает и разбирает std ОДИН раз на процесс и дальше
# судит файлы по одному (novac/src/main.nv, `check_path`: каждый файл — против
# тех же деклараций, свои строки, свой вердикт). Поэтому обычные фикстуры идут
# ПАКЕТОМ: чанки по CH файлов, J процессов в пуле, итог каждого файла
# собирается из строк пакета. Что НЕ меняется:
#   * фикстуры с `// NOVAC_PROGRAM` судятся по одной, как и прежде: их зовут
#     с NOVAC_SELF_PATH=<корень программы>, у каждой свой корень (дверь
#     novac_run_check);
#   * ЛЮБОЕ сомнение в пакете — и все файлы его чанка судятся по одному, как
#     прежде: код возврата вне {0,1,2} (падение, паника, снятие по времени),
#     непустой stderr, строка, которая не диагностика и не сообщение двери,
#     имя файла, которого нет в чанке, «худший» вердикт, не сходящийся с кодом
#     возврата, файл чанка без записи;
#   * СВЕРКА ВЫБОРКИ на каждом прогоне: каждая 40-я обычная фикстура судится
#     ещё и по одному, и её итог сверяется с пакетным ПОБАЙТОВО (out, err, rc).
#     Расхождение — весь пакетный результат выбрасывается, и обычные фикстуры
#     все судятся по одному (строка «ПАКЕТ РАСХОДИТСЯ» — в лог гейта).
# Так пакет не может сделать стража зелёным понарошку: он либо даёт тот же
# итог, что по одному, либо уступает дорогу по одному. Нет python или split —
# пакет не пробуется, всё судится по одному.
#
# Использование: sh scripts/tools/novac-check-cache.sh ROOT DIR [BIN]
export LC_ALL=C
ROOT="$(cd "${1:?ROOT}" && pwd)"
DIR="${2:?DIR}"
. "$(cd "$(dirname "$0")/../guards/lib" && pwd)/novac.sh"
BIN="${3:-$(novac_bin "$ROOT")}"
[ -f "$BIN" ] || { echo "novac-check-cache: нет бинаря $BIN — кэш не наполнен, стражи судят сами"; exit 0; }
mkdir -p "$DIR" || exit 1
printf '%s\n' "$BIN" > "$DIR/bin"
L="$DIR/list"
if [ -d "$ROOT/novac/fixtures" ]; then
    find "$ROOT/novac/fixtures" -type f -name '*.nv' | sort > "$L"
else
    : > "$L"
fi
one() {
    _e="$DIR/$(novac_check_key "$2")"
    printf '%s\n' "$2" > "$_e.path"
    # Через дверь вызова: вход программы (`// NOVAC_PROGRAM`) судится с корнем.
    novac_run_check "$BIN" "$2" "$_e.out" "$_e.err"
    echo "$?" > "$_e.rc"
}
J=$(novac_pool_jobs)
t0=$(date +%s)

# ---- пакет (№2020): обычные фикстуры — чанками, входы программ — по одной ----
B="$DIR/batch"
mkdir -p "$B/verify" || exit 1
: > "$B/plain"
: > "$B/fallback"
: > "$B/prog"
: > "$B/sample"
NPROG=0
while IFS= read -r _f; do
    novac_program_root "$_f"
    if [ -n "$NOVAC_PROG_ROOT" ]; then
        NPROG=$((NPROG + 1))
        printf '%s\n' "$_f" >> "$B/prog"
    else
        printf '%s\n' "$_f" >> "$B/plain"
    fi
done < "$L"
NP=$(grep -c '' "$B/plain")
# Интерпретатор — тот, что РАБОТАЕТ, а не первый по имени: `python3` на Windows
# может быть заглушкой Store («Python was not found»), и command -v её находит.
PY=""
for _cand in python3 python py; do
    if command -v "$_cand" >/dev/null 2>&1 && "$_cand" -c 'import json, re' >/dev/null 2>&1; then
        PY=$(command -v "$_cand")
        break
    fi
done

# Один чанк: один процесс `novac check` над своими файлами; код возврата — свой
# (не через xargs: его 123 съел бы разницу между «отказ» и «падение»).
batch_one() {
    # Номер чанка и его файл — ДО `set --`: та затирает позиционные параметры.
    _bi="$1"
    _bc="$2"
    set --
    while IFS= read -r _p; do set -- "$@" "$_p"; done < "$_bc"
    "$BIN" check "$@" > "$B/out.$_bi" 2> "$B/err.$_bi" </dev/null
    echo "$?" > "$B/rc.$_bi"
}

# Собрать итоги чанка из строк пакета. Выход 0 — каждая строка опознана, код
# возврата — худший из вердиктов, stderr пуст. Запись получает каждый файл
# чанка, КРОМЕ окрестности чужой строки (строка, приписанная другому файлу --
# телу метода std): кто из соседей её напечатал, пакет не знает, и эти
# несколько файлов уходят в $B/again.<номер>, чтобы судиться по одному.
# Иной код выхода — чанк мёртв, все его файлы идут по одному.
batch_split() {
    "$PY" - "$B/out.$1" "$2" "$DIR" "$B/rc.$1" "$B/err.$1" "$B/again.$1" <<'PYEOF'
import json, re, sys

out_path, list_path, cache_dir, rc_path, err_path, again_path = sys.argv[1:7]

def cksum(data):
    # POSIX cksum по схеме coreutils (crc.c): таблица 256 строк, на каждый байт
    # перемешивается СТАРШИЙ байт crc, затем длина — младшими байтами вперёд,
    # в конце — дополнение.
    tab = []
    for i in range(256):
        c = i << 24
        for _ in range(8):
            c = ((c << 1) ^ 0x04C11DB7) & 0xffffffff if c & 0x80000000 else (c << 1) & 0xffffffff
        tab.append(c)
    crc = 0
    for b in data:
        crc = ((crc << 8) & 0xffffffff) ^ tab[((crc >> 24) ^ b) & 0xff]
    n = len(data)
    while n:
        crc = ((crc << 8) & 0xffffffff) ^ tab[((crc >> 24) ^ (n & 0xff)) & 0xff]
        n >>= 8
    return crc ^ 0xffffffff

def norm(p):
    # Аргумент нативному бинарю Git Bash переписывает (/d/x -> D:/x), а файл в
    # строке — то, что бинарь получил; на Linux и в shell-поддевке — тождество.
    p = p.replace('\\', '/')
    if len(p) >= 3 and p[0] == '/' and p[2] == '/' and p[1].isalpha():
        p = p[1].upper() + ':' + p[2:]
    return p

files = [l.rstrip('\n').rstrip('\r') for l in open(list_path, encoding='utf-8', errors='surrogateescape') if l.strip()]
last = len(files) - 1
want = {norm(f): i for i, f in enumerate(files)}
texts = [[] for _ in files]
verdict = [0] * len(files)
rows = []          # (kind, file index | None, raw bytes); kind: own | foreign | door
worst = 0
bad = False
door = re.compile(r'^novac: (?:cannot read|not UTF-8): (.*)$')
with open(out_path, 'rb') as fh:
    for raw in fh.read().split(b'\n'):
        if raw == b'':
            continue
        # Байты строки хранятся как вышли (включая \r текстового режима Windows).
        line = raw.decode('utf-8', errors='surrogateescape').rstrip('\r')
        if line.startswith('{'):
            try:
                obj = json.loads(line)
                name = norm(obj['primary']['file'])
            except (ValueError, KeyError, TypeError):
                bad = True
                break
            if name in want:
                i = want[name]
                verdict[i] = 1
                texts[i].append(raw)
                rows.append(('own', i, raw))
                if worst < 1:
                    worst = 1
            else:
                # Строка, приписанная ДРУГОМУ файлу (тело метода std, #1521):
                # чей она вывод — решает окрестность ниже.
                rows.append(('foreign', None, raw))
                if worst < 1:
                    worst = 1
        else:
            m = door.match(line)
            if m is None or norm(m.group(1)) not in want:
                bad = True
                break
            i = want[norm(m.group(1))]
            verdict[i] = 2
            texts[i].append(raw)
            rows.append(('door', i, raw))
            worst = 2
if bad:
    sys.exit(3)
try:
    rc = int(open(rc_path, encoding='utf-8').read().strip())
except ValueError:
    sys.exit(3)
if rc not in (0, 1, 2) or rc != worst:
    sys.exit(3)
if open(err_path, 'rb').read() != b'':
    sys.exit(3)

# ЧЬЯ ЧУЖАЯ СТРОКА. Пакет судит файлы по порядку аргументов, и каждый печатает
# только во время своего суда. Чужая строка вышла из файла между последним, кто
# до неё назвал свою, и первым, кто назвал свою после неё (границы включены:
# своя и чужая строки одного файла чередуются по позиции). Кто между ними не
# напечатал ничего, мог быть причиной. Эта окрестность судится по одному.
uncertain = set()
for p, (kind, _i, _raw) in enumerate(rows):
    if kind != 'foreign':
        continue
    a = next((rows[q][1] for q in range(p - 1, -1, -1) if rows[q][0] != 'foreign'), 0)
    b = next((rows[q][1] for q in range(p + 1, len(rows)) if rows[q][0] != 'foreign'), last)
    for i in range(a, b + 1):
        uncertain.add(i)

with open(again_path, 'wb') as o:
    for i in sorted(uncertain):
        fb = files[i].encode('utf-8', errors='surrogateescape')
        o.write(fb + b'\n')
for i, f in enumerate(files):
    if i in uncertain:
        continue
    fb = f.encode('utf-8', errors='surrogateescape')
    base = '%s/%d-%d' % (cache_dir, cksum(fb), len(fb))
    with open(base + '.path', 'wb') as o:
        o.write(fb + b'\n')
    with open(base + '.out', 'wb') as o:
        for t in texts[i]:
            o.write(t + b'\n')
    with open(base + '.err', 'wb') as o:
        o.write(b'')
    with open(base + '.rc', 'wb') as o:
        o.write(b'%d\n' % verdict[i])
PYEOF
}

verify_one() {
    _e="$B/verify/$(novac_check_key "$2")"
    printf '%s\n' "$2" > "$_e.path"
    novac_run_check "$BIN" "$2" "$_e.out" "$_e.err"
    echo "$?" > "$_e.rc"
}

BY_BATCH=0
DIVERGED=0
BATCHED=0
NSKIP=0
if [ "$NP" -gt 0 ] && [ -n "$PY" ] && command -v split >/dev/null 2>&1; then
    # Пять раундов по J чанков: старт процесса платится ~5 раз на ядро, а
    # последний раунд не остаётся один на всех.
    CH=$(( (NP + J * 5 - 1) / (J * 5) ))
    [ "$CH" -ge 8 ] || CH=8
    rm -f "$B"/chunk.*
    split -l "$CH" "$B/plain" "$B/chunk."
    : > "$B/chunks"
    for _c in "$B"/chunk.*; do
        [ -f "$_c" ] && printf '%s\n' "$_c" >> "$B/chunks"
    done
    BATCHED=1
    novac_pool batch_one "$B/chunks" "$J"
    _n=0
    while IFS= read -r _c; do
        _n=$((_n + 1))
        : > "$B/again.$_n"
        if [ -f "$B/rc.$_n" ] && batch_split "$_n" "$_c"; then
            # Окрестность чужой строки — по одному; счёт пакетных без неё.
            cat "$B/again.$_n" >> "$B/fallback"
            BY_BATCH=$((BY_BATCH + $(grep -c '' "$_c") - $(grep -c '' "$B/again.$_n")))
            # Запись есть у каждой фикстуры живого чанка — или её путь
            # в откат. Только ЖИВОГО: файлы мёртвого чанка уже там, второй раз
            # их не пишем (иначе чанк судится по одному дважды).
            while IFS= read -r _f; do
                [ -f "$DIR/$(novac_check_key "$_f").rc" ] || printf '%s\n' "$_f" >> "$B/fallback"
            done < "$_c"
        else
            # Чанк мёртв (пакет не судил или судил не так) — каждый его файл по одному.
            while IFS= read -r _f; do printf '%s\n' "$_f" >> "$B/fallback"; done < "$_c"
        fi
    done < "$B/chunks"
    # Сверка выборки: каждая 40-я — по одному, в отдельный каталог; побайтово.
    _i=0
    while IFS= read -r _f; do
        _i=$((_i + 1))
        [ $(( _i % 40 )) -eq 1 ] && printf '%s\n' "$_f" >> "$B/sample"
    done < "$B/plain"
    novac_pool verify_one "$B/sample" "$J"
    while IFS= read -r _f; do
        _k="$(novac_check_key "$_f")"
        if [ ! -f "$DIR/$_k.rc" ]; then
            # Файл мёртвого чанка: пакетной записи у него нет и не должно быть,
            # он уже судится по одному, — сверять не с чем.
            NSKIP=$((NSKIP + 1))
            continue
        fi
        if ! cmp -s "$DIR/$_k.out" "$B/verify/$_k.out" \
           || ! cmp -s "$DIR/$_k.err" "$B/verify/$_k.err" \
           || ! cmp -s "$DIR/$_k.rc" "$B/verify/$_k.rc"; then
            DIVERGED=$((DIVERGED + 1))
            echo "novac-check-cache: сверка выборки РАСХОДИТСЯ: $_f"
        fi
    done < "$B/sample"
    if [ "$DIVERGED" -ne 0 ]; then
        # Пакет не равен по одному — его итог никому не достаётся.
        echo "novac-check-cache: ПАКЕТ РАСХОДИТСЯ ($DIVERGED из выборки) — обычные фикстуры судятся по одному"
        while IFS= read -r _f; do
            rm -f "$DIR/$(novac_check_key "$_f").path" "$DIR/$(novac_check_key "$_f").out" \
                  "$DIR/$(novac_check_key "$_f").err" "$DIR/$(novac_check_key "$_f").rc"
        done < "$B/plain"
        # Все обычные — по одному; то, что уже стояло в откате (окрестности,
        # мёртвые чанки), входит в них: не судим одно дважды.
        cp "$B/plain" "$B/fallback"
        BY_BATCH=0
    fi
fi
if [ "$BATCHED" -eq 0 ]; then
    # Пакет не пробовался — обычные фикстуры судятся по одному, как прежде.
    cat "$B/plain" >> "$B/fallback"
fi
# Откат = то, что пакет не судил, плюс входы программ; каждый файл ОДИН раз
# (окрестности разных чужих строк перекрываются).
sort -u "$B/fallback" "$B/prog" > "$B/fallback.u" || exit 1
mv "$B/fallback.u" "$B/fallback"
if [ -s "$B/fallback" ]; then
    novac_pool one "$B/fallback" "$J"
fi
NF=$(grep -c '' "$B/fallback")
NS=$(grep -c '' "$B/sample")
echo "novac-check-cache: пакетом $BY_BATCH, по одному $NF (входы программ $NPROG), сверка выборки: $NS шт. (в откате $NSKIP), расхождений $DIVERGED"
n=$(grep -c '' "$L")
m=$(find "$DIR" -maxdepth 1 -name '*.rc' | wc -l | tr -d ' ')
echo "novac-check-cache: фикстур $n, записей $m, стена $(( $(date +%s) - t0 ))с, потоков $J"
