# Задание: диагноз коллизии NovaValue_Output (юзер value-enum vs std value-record, items 21/14/22)
T: репро → корень → бриф дизайн-прохода; сам фикс — отдельное решение
«бери все это по рекомендации, в том числе Что НЕ быстрое» (владелец, 2026-11-03, этот чат)

## Результат (диагноз, фикс НЕ делался — дизайн-проход)
Корень: emit_c.rs record_def (~:6344) — type_def_modules наполняется ТОЛЬКО
peer-файлами CU; std-типы не участвуют, коллизия юзер-vs-std не детектится,
оба типа манглятся NovaValue_Output → struct одного теряется (incomplete).
Фикс = снять exclusion value-record оси + D381-квалификация по модулям;
blast radius: ABI-нейминг, обязателен байт-дифф на бесколлизионных деревьях.
Спайк: spikes/env/repro_output_c_collision/ (product repo: canopus)
(закоммичен 9b6ccd3f в 268-линии). Полный бриф — wave-status.md.
