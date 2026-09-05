# -*- coding: utf-8 -*-
# usage: python -X utf8 count_idx_chk.py <kept .c file>
# Prints, per parser function body in the generated C, the number of `nova_idx_chk` calls.
import re, sys
src = open(sys.argv[1], encoding="utf-8", errors="replace").read()
names = "to_f64|parse_int_core|parse_uint_core|to_int|to_uint|to_i8|to_i16|to_i32|to_i64|to_u8|to_u16|to_u32|to_u64|to_bool"
for m in re.finditer(r'\n([^\n;{}]*?\b(\w*(?:%s)\w*)\s*\([^;{}]*\)\s*\{\n)(.*?)\n\}' % names, src, re.S):
    print(m.group(2), m.group(3).count("nova_idx_chk"))
