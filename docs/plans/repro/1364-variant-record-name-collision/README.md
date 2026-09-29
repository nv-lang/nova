# №1364 — вариант enum не коэрсится к своему enum, если в ДРУГОМ модуле есть запись с тем же именем

Найдено окном nova-postgres (Kim Code), срез 286.1, 2026-09-28; перепроверено
интегратором 2026-09-29 в обе стороны.

Суффикс `.nv.txt` — улика вне раннера. Для прогона собрать пакет из трёх файлов
без `.txt` (`nova.toml`, `src/core.nv`, `src/main.nv`) в любом каталоге вне репы и:

```sh
nova-cli/target/release/nova.exe build <каталог>/src/main.nv -o <куда-нибудь>.exe
```

| проба | `build` |
|---|---|
| как есть: запись `Notification` в `main`, вариант `Event.Notification` в `core` | `[E7301] cannot pass value of type \`Event.Notification\` as argument \`v\` of type \`Event\`` (core.nv:11) |
| контроль: запись переименована в `Note`, больше ничего | собрано, печатает `ok 1` |
