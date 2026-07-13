# known_red — карантин вечно-красных носителей (вне дефолт-гейта)

Дисциплина «красный conformance = стоп» требует, чтобы дефолтный прогон
`spec_tests/conformance` был строго зелёным. Тесты здесь — известные красные
с открытыми маркерами; возвращаются в conformance при закрытии маркера:

- `t4_sqlite_e2e_ok.nv` — FFI-шим mini_sqlite не линкуется standalone (198-notes Ф.4c).
- `view_descriptor_stack.nv` — стек-дескриптор аллоцирует (172.14, known-red).
