# nova_tests/plan84 — относительные импорты `./` / `../` (Plan 84)

Фикстуры для package-scoped относительных импортов (D29 rev-4).

## Позитивные

- `sibling/use_sibling_test.nv` — `import ./helper` (сосед, single-file).
- `parent/sub/use_parent_test.nv` — `import ../shared` (модуль уровнем выше).
- `fmod_rel/use_fmod_test.nv` — `import ./geo` (цель — folder-module).
- `reexport/use_reexport_test.nv` — `import ./facade`, где `facade.nv`
  делает `export import ./inner` (относительная резолюция при re-export).

## Негативные (EXPECT_COMPILE_ERROR)

- `neg_escape_test.nv` — `../../` выходит за границу пакета `nova_tests/`.
- `neg_notfound/nf_test.nv` — относительный импорт несуществующего модуля.
- `neg_dotdot_test.nv` — `.././` — `./` не может сопровождаться `../`.

## Аудит взаимодействий (Ф.4)

- folder-module как цель `./` — `fmod_rel`.
- `export import ./X` re-export — `reexport`.
- `internal/` rule H + cycle-detection — относительный импорт резолвится
  в canonical path, поэтому правило H (`find_internal_owner_dir`) и
  cycle-detection (visited по canonical) применяются без изменений.
