#!/bin/sh
# .p259/mkcorpus.sh N  — собрать подкорпус из первых N top-level .nv
# spec_tests/conformance (алфавитно), переименовав объявление модуля.
# Каталог: spec_tests/p259corpus/n<N>/  (untracked, чистится cleancorpus.sh).
#
# ПОЧЕМУ ВНУТРИ spec_tests/: часть фикстур импортирует соседние подмодули
# `spec_tests.conformance.*`; вне пакета spec_tests такой импорт отвергается
# как межпакетный (Plan 03.1 §3.2).
#
# ЗАЧЕМ peer-файл `a__p259_imports.nv`: в полном корпусе импорт std-модуля
# может лежать в ЛЮБОМ peer'е (folder-module = один модуль из co-equal
# файлов, импорты общие). Срез первых N файлов теряет часть импортов и
# ломает компиляцию не по своей вине. Peer с объединением всех std-импортов
# корпуса восстанавливает исходную видимость.
set -eu
N="$1"
ROOT="/d/Sources/nv-lang/nova-p259"
DST="$ROOT/spec_tests/p259c_n$N"
rm -rf "$DST"
mkdir -p "$DST"
{
    echo "module spec_tests.p259c_n$N"
    cat "$ROOT/.p259/imports_std.txt"
} > "$DST/a__p259_imports.nv"
# Ассеты фикстур (embed): не-.nv файлы верхнего уровня + каталоги без .nv.
cp "$ROOT"/spec_tests/conformance/*.bin "$ROOT"/spec_tests/conformance/*.txt "$DST/" 2>/dev/null || true
for d in d412d_dir d412e_glob_dir d412e_hidden_dir; do
    [ -d "$ROOT/spec_tests/conformance/$d" ] && cp -r "$ROOT/spec_tests/conformance/$d" "$DST/"
done
i=0
for f in $(ls "$ROOT"/spec_tests/conformance/*.nv | sort); do
    case "$(basename "$f")" in *_slow.nv) continue;; esac
    i=$((i+1))
    [ "$i" -le "$N" ] || break
    sed 's/^module spec_tests\.conformance$/module spec_tests.p259c_n'"$N"'/' "$f" \
        > "$DST/$(basename "$f")"
done
echo "corpus n$N: $(ls "$DST" | wc -l) файлов, $(cat "$DST"/*.nv | wc -c) байт"
