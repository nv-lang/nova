#!/bin/sh
# №1872 — воспроизведение класса «no member named 'ctx'» (сырая печать имён из
# C_NAME_MEMBERS в эмиссии novac). Проба ЗАПУСКАЕТСЯ (№695 п. 2а), потому `.nv`.
#
# Прогон через смоук (ДО фикса — красный на этапе clang, ПОСЛЕ — зелёный):
#   sh scripts/tools/novac-e1-smoke.sh docs/plans/repro/1872/self_field_ctx.nv
#
# Вручную, против любого бинаря novac:
#   novac check docs/plans/repro/1872/self_field_ctx.nv        # rc=0 и до, и после
#   novac emit  docs/plans/repro/1872/self_field_ctx.nv > /tmp/1872.c
#   # ДО фикса в /tmp/1872.c: `(_novac_self->ctx)` при `Nova_Ctx* nv_ctx;` в struct,
#   # и `_novac_tmp_tN->nv_ctx = ctx;` при локале `nv_ctx` — clang даёт
#   #   error: no member named 'ctx' in 'struct Nova_Box'
#   #   error: use of undeclared identifier 'ctx'
# Снимок ДО-эмиссии: self_field_ctx.before.c.txt (23136 строк, novac из 3f4b68a8b).
set -u
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
exec sh "$ROOT/scripts/tools/novac-e1-smoke.sh" "$ROOT/docs/plans/repro/1872/self_field_ctx.nv"
