# known_red — карантин вечно-красных носителей (вне дефолт-гейта)

Дисциплина «красный conformance = стоп» требует, чтобы дефолтный прогон
`spec_tests/conformance` был строго зелёным. Тесты здесь — известные красные
с открытыми маркерами; возвращаются в conformance при закрытии маркера:

- `t4_sqlite_e2e_ok.nv` — FFI-шим mini_sqlite не линкуется standalone (198-notes Ф.4c).
- `view_descriptor_stack.nv` — стек-дескриптор аллоцирует (172.14, known-red).
- `local_shadows_topfn/` — `[M-198-f4c-2-local-not-shadow-crossfile-topfn]`:
  локальная переменная не затеняет top-level `fn` того же имени из ДРУГОГО файла
  того же folder-module (re-verified 2026-07-17, Plan 212 pt.7).
- `p196_b10m_phase1c_probe.nv` — `[M-196-probes-b10m-phase1c]`: phase-1c
  pre-scan репро для `B10m_ident_empty_fallback` (emit_c.rs
  `infer_call_ret_c`) — module-level `ro y = helper(21)` форвард-ссылается
  на expr-body free fn без явного `-> T`, чей тело зовёт ДРУГУЮ такую же
  fn — CC-FAIL (`assigning to 'nova_unit' from incompatible type
  'nova_int'`). Детали: docs/plans/wip/196-probes-notes.md (Plan 196
  волна-1 разведка-зонд).
- `p196_b11al_terminal_probe.nv` / `p196_b12q_terminal_probe.nv` /
  `p196_b12r_terminal_probe.nv` / `p196_b12s_terminal_probe.nv` —
  `[M-196-probes-terminal-*]`: red-зонды для 4 терминалов `infer_call_ret_c`
  (`B11al_panic_method_p67`/`B12q_panic_path_p67`/`B12r_panic_path_no_method_seg`/
  `B12s_panic_path_no_parts`) — ЖЁСТКИЕ ПАНИКИ компилятора (`nova: internal
  error`, exit=101), НЕ перехватываются `nova test`'ом (весь процесс
  обрывается без summary, даже для одного файла) — **никогда не гонять эти
  4 файла в одном вызове `nova test` с другими файлами** (сама паника молча
  съедает результаты остальных). Детали механики каждого + нужный
  чекер-фикс: docs/plans/wip/196-probes-notes.md §2 (Plan 196 волна-1
  разведка-зонд).
