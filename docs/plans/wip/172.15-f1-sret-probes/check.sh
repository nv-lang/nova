#!/usr/bin/env bash
# Plan 172.15 Ф.1 — приёмочный прогон фикстур периметра `__sret`.
# Каждая фикстура изолирует ОДНУ снятую жёсткую привязку; проба
# «подсунь негодное» ломает соответствующую часть предиката и обязана
# покраснить ровно её строку.
#
# Использование: bash check.sh          (собрать и проверить всё)
set -u
export NOVA_GC_LIB_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib"
export NOVA_GC_INCLUDE_DIR="D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include"
NOVA=${NOVA:-d:/Sources/nv-lang/nova-p172sret/nova-cli/target/release/nova.exe}
cd "$(dirname "$0")" || exit 1

pass=0; fail=0
say() { # say <NAME> <ok|no> <detail>
  if [ "$2" = ok ]; then echo "  ok   $1: $3"; pass=$((pass+1));
  else echo "  FAIL $1: $3"; fail=$((fail+1)); fi
}
want() { # want <NAME> <file> <regex> <detail>
  if grep -qE "$3" "$2"; then say "$1" ok "$4"; else say "$1" no "$4"; fi
}

build() { # build <stem> -> echoes path to .c
  "$NOVA" build "$1.nv" -o "$1" --keep-artifacts >/dev/null 2>&1 || { echo ""; return; }
  find "$(ls -dt /tmp/nova_tests-*/ | head -1)" -name "$1.c" 2>/dev/null | head -1
}

echo "== 172.15 Ф.1: фикстуры периметра __sret =="

C=$(build f_bytes); CB="$C"
if [ -z "$C" ]; then say ACCEPT no "f_bytes.nv не собралась"; else
  want ACCEPT-1 "$C" 'Nova_str_method_bytes__sret\(nova_str nova_self, Nova_Vec____nova_byte\* _out\)' \
       "bytes() имеет __sret-вариант"
  want ACCEPT-2 "$C" 'Nova_Vec____nova_byte _nv_tmp_[0-9]+ = \{0\};' \
       "буфер результата — слот в кадре ВЫЗЫВАЮЩЕГО"
  want ACCEPT-3 "$C" 'b = Nova_str_method_bytes__sret\(s, \(&_nv_tmp_[0-9]+\)\)' \
       "вызов идёт в __sret с адресом слота"
  want ACCEPT-4 "$C" '_nv_tmp_[0-9]+ = Nova_Vec____nova_byte_static_new__const_nova_byte_p_nova_int__sret\(.*, _out\)' \
       "цепочка bytes -> []u8.new пробрасывает _out (ось «обёртка unsafe»)"
  if grep -A8 -F 'nova_byte_static_new__const_nova_byte_p_nova_int__sret(const' "$C" \
     | grep -qF 'nova_alloc'; then say ACCEPT-5 no "в цепочке осталась аллокация";
  else say ACCEPT-5 ok "во всей цепочке bytes() НЕТ nova_alloc"; fi
fi

C1=$(build f1_alias); C2=$(build f2_vec)
if [ -z "$C1" ] || [ -z "$C2" ]; then say ALIAS no "фикстуры оси «алиас» не собрались"; else
  want ALIAS-1 "$C1" 'Nova_Src_method_view__sret\(Nova_Src\* nova_self, Nova_Vec____nova_int\* _out\)' \
       "спеллинг []T получает __sret"
  want ALIAS-2 "$C2" 'Nova_Src_method_view__sret\(Nova_Src\* nova_self, Nova_Vec____nova_int\* _out\)' \
       "спеллинг Vec[T] получает __sret"
  if diff <(sed -e 's/f1_alias/FIX/g' -e '/SRC:/d' "$C1") \
          <(sed -e 's/f2_vec/FIX/g'   -e '/SRC:/d' "$C2") >/dev/null; then
    say ALIAS-3 ok "[]T и Vec[T] дают ПОБАЙТНО один C"
  else say ALIAS-3 no "[]T и Vec[T] дают разный C"; fi
fi

C=$(build f5_unsafe)
if [ -z "$C" ]; then say UNSAFE no "f5_unsafe.nv не собралась"; else
  want UNSAFE-1 "$C" 'Nova_Holder_method_view__sret\(Nova_Holder\* nova_self, Nova_Vec____nova_byte\* _out\)' \
       "тело под unsafe { } получает __sret"
  want UNSAFE-2 "$C" '_nv_tmp_[0-9]+ = Nova_Vec____nova_byte_static_new__const_nova_byte_p_nova_int__sret\(.*, _out\)' \
       "проброс _out сквозь обёртку unsafe { }"
fi

C=$(build f_record)
if [ -z "$C" ]; then say TYPENAME no "f_record.nv не собралась"; else
  want TYPE-1 "$C" 'Nova_Pair_static_of2__sret\(nova_int x, Nova_Pair\* _out\)' \
       "НЕ-Vec кучевая запись получает __sret (имя C-типа не решает)"
  want TYPE-2 "$C" 'Nova_Pair\* _nv_tmp_[0-9]+ = _out;' \
       "тело пишет по месту, без nova_alloc"
fi

# КОНТРОЛЬ: доснятый-хардкод сайт (`Vec[T] @index(r Range)` — именованный
# `Self { … }`-литерал), работавший и ДО 172.15. Обязан оставаться зелёным во
# ВСЕХ пробах «подсунь негодное» — иначе проба сломала не то, что метила.
if [ -n "${CB:-}" ]; then
  want CONTROL "$CB" 'Vec____nova_byte_method_index__NovaValue_Range__sret\(Nova_Vec____nova_byte\* nova_self, NovaValue_Range r, Nova_Vec____nova_byte\* _out\)' \
       "прежний Leaf-сайт (Vec @index(Range)) жив"
fi

echo "PASS: $pass FAIL: $fail"
[ "$fail" -eq 0 ]
