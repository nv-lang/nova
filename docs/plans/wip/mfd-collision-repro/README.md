# Репро [M-mono-fn-decls-module-qualified-key] (№129 Task C, откачено приёмкой)

Три файла — репро тихого miscompile: два модуля со своей module-private
`fn[T] helper(...)` (111/222), вызов из модуля A возвращает тело B. Жёсткая
insert-time ошибка ОТКАЧЕНА 2026-07-30: красила легальный корпус (два
фикстурных `check_ok` в мега-CU). Файлы паркуются ЗДЕСЬ (не в conformance)
до окна правильного фикса — module-qualified ключ mono_fn_decls; тогда
mfd_entry становится pos-фикстурой (111 == 111).
