# План 200 Пункт 18 — UTF-8 лестница (чекпоинт)

## Статус: БЛОКИРОВАН

### Выполнено
1. ✅ Создан worktree p200-18-utf8
2. ✅ Добавлен метод `char @encode_utf8() -> (int, [4]u8)` в defaults.nv с лестницей из string_builder.nv
3. ✅ Переписан @len_utf8() делегатом на .0
4. ✅ Обновлена string_builder.nv:
   - Удалены char_utf8_len (~281) и char_utf8_bytes (~289)
   - Обновлены использования в pad_in_place (строка 325-326)
   - Переписан @append(c char) на encode_utf8()
5. ✅ Обновлена write_buffer.nv:
   - Переписан @write_char на encode_utf8()

### Проблема

Компилятор Nova генерирует **неправильный порядок typedef'ов** для кортежей с фиксированными массивами:
- Typedef для `_NovaTuple_2_8_nova_int_25__NovaFixArr_4_9_nova_byte` использует `_NovaFixArr_4_9_nova_byte`
- Но определение `_NovaFixArr_4_9_nova_byte` находится ПОСЛЕ использования
- Результат: ошибка компилятора C `unknown type name '_NovaFixArr_4_9_nova_byte'` (строка 216)

```c
// НЕПРАВИЛЬНЫЙ ПОРЯДОК:
#ifndef NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_int_25__NovaFixArr_4_9_nova_byte
#define NOVA_TUPLE_TYPEDEF__NovaTuple_2_8_nova_int_25__NovaFixArr_4_9_nova_byte
typedef struct ... { nova_int f0; _NovaFixArr_4_9_nova_byte f1; } ...;  // ОШИБКА: тип не определён!
#endif
...
#ifndef NOVA_FIXARR_TYPEDEF__NovaFixArr_4_9_nova_byte
#define NOVA_FIXARR_TYPEDEF__NovaFixArr_4_9_nova_byte
typedef struct _NovaFixArr_4_9_nova_byte { nova_byte data[4]; } ...;  // Определение ПОЗЖЕ
#endif
```

### Попытки решения

1. Переделал возврат кортежа с `(len, out)` в промежуточных ветках на единственное `(len, out)` в конце функции
   - Не помогло: компилятор всё равно генерирует typedef неправильно

### Выводы

- Кортежи с фиксированными массивами (type, [N]element) в Nova имеют баг в C-codegen
- Это не проблема синтаксиса Nova, а проблема генератора typedef'ов
- Дизайн в HEAD worktree указывает на это как финализированное решение, но компилятор не готов

### Дальнейшие действия

Нужно либо:
1. Поправить компилятор Nova (C-codegen typedef ordering)
2. Найти альтернативный способ представления (record вместо tuple?)
3. Получить подтверждение от владельца
