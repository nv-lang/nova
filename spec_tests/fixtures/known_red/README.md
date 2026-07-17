# known_red — карантин вечно-красных носителей (вне дефолт-гейта)

Дисциплина «красный conformance = стоп» требует, чтобы дефолтный прогон
`spec_tests/conformance` был строго зелёным. Тесты здесь — известные красные
с открытыми маркерами; возвращаются в conformance при закрытии маркера:

- `t4_sqlite_e2e_ok.nv` — FFI-шим mini_sqlite не линкуется standalone (198-notes Ф.4c).
- `view_descriptor_stack.nv` — стек-дескриптор аллоцирует (172.14, known-red).
- `privtype_file_discrimination/` — `[M-198-f4c-1-privfile-type-not-discriminated]`:
  priv(file) ТИПЫ (в отличие от fn/method) не файл-дискриминируются в checker-резолве
  (re-verified 2026-07-17, Plan 212 pt.7).
- `local_shadows_topfn/` — `[M-198-f4c-2-local-not-shadow-crossfile-topfn]`:
  локальная переменная не затеняет top-level `fn` того же имени из ДРУГОГО файла
  того же folder-module (re-verified 2026-07-17, Plan 212 pt.7).
