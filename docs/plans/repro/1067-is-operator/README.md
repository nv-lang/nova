# Пробы к оператору `is` (D54): восемь форм из спеки против оракула, 2026-09-10

Заведено исследовательским окном по вопросу владельца «`is` в std почти не встречается, думаю
там куча нестыковок». Восемь однофайловых проб, каждая — одна форма из D54 (`03-syntax.md`,
раздел «`is` — runtime type-check»). Все запуски — `nova check`, для прошедших чекер — `nova build`
+ исполнение.

| проба | форма (D54) | что спека обещает | `nova check` | `nova build` / run |
|---|---|---|---|---|
| `a_match_is` | `match x { n is int => …; is str => … }` | сценарий 1, «pattern в match, биндинг + smart cast» | **parse error** `expected =>, got is` | — |
| `b_generic_is` | `fn[T] f(x T) => x is int` | «на остальных типах — ошибка компиляции» | **ok** | **codegen error** «`is int` — unknown variant» |
| `c_slice_is` | `arr []int; arr is int` | то же — ошибка компиляции | **ok** | **codegen error** «`is int` — unknown variant» |
| `d_newtype_is` | `type Wrapped Shape; w is Circle` | спека молчит про newtype над суммой | **ok** | **ошибка компилятора C**: `member reference type 'NovaValue_Shape' is not a pointer` |
| `e_sum_bind_is` | `if r is Ok(n)` | «ошибка, нужно `if Ok(n) = r`» | ошибка, но **не та**: `undefined identifier n` дважды, вторая — со span `1:1` | — |
| `f_any_as` | `x.as[int]!!` (`any.as[T]() Fail[TypeMismatch]`) | «cast через Fail, для строгих случаев» | **parse error** `expected field name or index after .` | — |
| `g_sum_is_ok` | `r is Ok`, `o is None`, `s is Origin`, `s is Circle` | сценарий 2, variant-check | ok | `true false true true false` — **верно** |
| `h_record_is` | `u User; u is int` | ошибка компиляции | `E_IS_NON_ANY` — **верно**, текст упоминает и any, и sum | — |

Что работает и пришпилено фикстурой: `any is T`, smart-cast в `if`, `try_as[T]()`
(`spec_tests/conformance/any_is/box_is_downcast.nv`), variant-check на суммах (g; в std —
`hex_test`, `ini_test`, `ulid_test`, `uuid_test`, `snowflake_test`: `Err(e) => assert(e is X)`,
`nova test std --filter hex` → PASS).

## Как запускать

```sh
for f in a_match_is b_generic_is c_slice_is d_newtype_is e_sum_bind_is f_any_as g_sum_is_ok h_record_is; do
  cp docs/plans/repro/1067-is-operator/$f.nv.txt /tmp/$f.nv
  nova-cli/target/release/nova.exe check /tmp/$f.nv
done
nova-cli/target/release/nova.exe build /tmp/d_newtype_is.nv -o /tmp/d.exe   # ошибка на уровне C
```
