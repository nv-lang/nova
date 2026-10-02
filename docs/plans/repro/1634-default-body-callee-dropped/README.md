# Метод, который зовёт только тело протокола по умолчанию, вычищается DCE

Реестр 221.1, №1634 (сосед #1414, #1011). Найдено помощником nova-88 2026-10-02
при приёмке №1598.

## Пробы

* `dm_direct.nv.txt` — value-запись, вызов `s.twice()` напрямую;
* `dm_bound.nv.txt` — тот же вызов через границу обобщения `[T DbcArea]`.

Переименовать в `.nv` (в своём каталоге: пиры одного каталога — один модуль) и
собрать оракулом до фикса:

```sh
nova build dm_direct.nv -o dm.exe
```

## Что было до фикса

```
lld-link: error: undefined symbol: Nova_Small_method_area
```

Для именованного кортежа тот же случай падает раньше линковки:

```
[INTERNAL-PANIC] [E_CODEGEN_TYPE_UNKNOWN] method call `.area` return type unknown; obj_ty="NovaTuple_DbcPair" obj=Other
```

Контроль: heap-запись (`type Small { ... }` без `value`) падает так же — дело не
в ABI приёмника.

## Корень и фикс

`collect_used_names` (`compiler-codegen/src/lints.rs`) в ветке
`TypeDeclKind::Protocol` не обходил `default_body`; метод-уровневый DCE считал
`area` мёртвым. Фикс — обход тела, контрактов, умолчаний параметров и границ
обобщений метода протокола. Фикстура:
`spec_tests/conformance/standalone/default_body_callee_reachable.nv`.
