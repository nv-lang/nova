<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Не пара проба/контроль — это ДВЕ отдельные пробы

`control.nv` (`#impl(Equal)` на записи) действительно чист — это подтверждает,
что путь построения `Equal` для записи работает. `p.nv` (`type MyInt = int`)
даёт ПАРСЕР-ОШИБКУ, а не диагностику типа `E_AUTO_DERIVE_UNSUPPORTED_KIND`:
такого синтаксиса объявления типа в Nova нет вовсе (`expected type, got '='`).

Значит форма для `E_AUTO_DERIVE_UNSUPPORTED_KIND` НЕ НАЙДЕНА в этой волне —
подробности в `../../batch2-results.txt`, раздел «ТРЕТЬЯ ПОПЫТКА».
