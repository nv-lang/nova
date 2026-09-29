# №1367 — текст диагностики `D133-not-consumed` и соседних: русский, пустой тип, совет удалённого синтаксиса

Носитель — `main.nv.txt` (проба окна nova-postgres `repro-local/enum_consume_socket`).
Сама диагностика здесь ПРАВА по существу: `boxed` передан в не-consume параметр
`mut b Boxed` и не потреблён до конца области. Дефект — только ТЕКСТ.

```sh
nova-cli/target/release/nova.exe build <каталог>/src/main.nv -o <куда-нибудь>.exe
```

Вывод интегратора 2026-09-29:

```
main.nv:36:33: error: [D133-not-consumed] переменная `boxed` (тип ``) не consumed до scope-exit.
Добавьте вызов одного из: объявите consume-метод для этого типа, либо `return boxed`, либо передайте в consume-param.
```

Три дефекта текста, место каждого прочитано в исходнике:

1. русский текст диагностики — `compiler-codegen/src/types/mod.rs:44620-44622`
   (правило репы: diagnostic texts in English);
2. `(тип ``)` — имя consume-enum `Boxed` не доходит до сообщения;
3. совет удалённого D189 синтаксиса: `types/mod.rs:44678`
   («suggestion: add errdefer + okdefer для полного покрытия») и генератор
   документации `compiler-codegen/src/doc/render_md.rs:331` и `:333` (каждая
   сгенерированная страница consume-типа советует `errdefer`/`okdefer`).
