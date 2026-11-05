# Задание: io.read_line[R Read] — построчное чтение по Read-протоколу (item 31)
T: репро → фикс → батарея → пин → ветка+пуш
«согласовано, сделай супер быстро» (владелец, 2026-11-02, этот чат)

## Результат
Выполнено: std/src/io/core.nv `read_line[R Read]` (newline съедается, byte-wise
1-byte reads сохраняют fill-контракт read_in, InvalidData на невалидном UTF-8)
+ 3 mock-теста. Коммит 5c7223811, ветка fix/013bc-stdin-and-r3 (запушена).
Батарея: io-суит PASS, unit 1241. Item 31 (spikes/env item 31) закрыт.
