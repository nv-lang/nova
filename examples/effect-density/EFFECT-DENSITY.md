# Плотность эффектов в реальном Nova-коде

Этот пример проверяет беспокойство: «в Nova получится по десятку эффектов
в каждой функции — сигнатуры станут нечитаемыми». Реализован типовой
backend money-transfer сервиса со всеми обычными production-concerns:
БД, кэш, лог, аутентификация, идемпотентность, метрики, distributed
trace, генерация id, часы.

## Состав примера

| Файл | Слой | Что внутри |
|---|---|---|
| `domain.nv` | данные + эффекты | 9 эффектов, 5 типов |
| `repository.nv` | доступ к БД | 4 public + 3 private |
| `service.nv` | бизнес-логика | 2 public + 5 private |
| `http.nv` | HTTP handler'ы | 3 public + 4 private |
| `main.nv` | сборка handler-стека, запуск | 1 public + 1 private + handler factories |

## Подсчёт плотности эффектов в сигнатурах

В таблице — все функции примера. Колонки:

- **Eff** — количество эффектов в сигнатуре (включая `Fail[E]`)
- **Видимость** — `export` (явно) или `private` (выводится компилятором, D28)
- **Эффекты** — список через пробел

Сортировка: по слою сверху вниз, внутри слоя — по плотности.

### Repository

| Функция | Eff | Видимость | Эффекты |
|---|---:|---|---|
| `account_get` | 3 | export | `Db Cache Fail[RepoError]` |
| `account_update_balance` | 3 | export | `Db Cache Fail[RepoError]` |
| `transfer_insert` | 2 | export | `Db Fail[RepoError]` |
| `transfer_update_status` | 2 | export | `Db Fail[RepoError]` |
| `row_to_account` | 0 | private | — |
| `encode_account` | 0 | private | — |
| `decode_account` | 0 | private | — |

### Service

| Функция | Eff | Видимость | Эффекты |
|---|---:|---|---|
| `transfer_money` | **10** | export | `Db Cache Logger Clock IdGen AuthContext Metrics Trace Idempotency Fail[TransferError]` |
| `account_view` | 4 | export | `Db Cache AuthContext Fail[TransferError]` |
| `do_transfer` | 7 | private (выводится) | `Db Cache Logger Clock IdGen Metrics Fail[TransferError]` |
| `validate_amount` | 1 | private | `Fail[TransferError]` |
| `validate_pair` | 1 | private | `Fail[TransferError]` |
| `add_money` | 1 | private | `Fail[TransferError]` |
| `sub_money` | 1 | private | `Fail[TransferError]` |
| `decode_transfer` | 1 | private | `Fail[TransferError]` |

### HTTP

| Функция | Eff | Видимость | Эффекты |
|---|---:|---|---|
| `handle_create_transfer` | **10** | export | `Db Cache Logger Clock IdGen AuthContext Metrics Trace Idempotency Fail[HttpError]` |
| `handle_get_account` | 4 | export | `Db Cache AuthContext Fail[HttpError]` |
| `handle_health` | 1 | export | `Db` |
| `parse_transfer_request` | 1 | private | `Fail[ParseError]` |
| `parse_id` | 1 | private | `Fail[HttpError]` |
| `transfer_dto` | 0 | private | — |
| `to_http_error` | 0 | private | — |

### Main / wiring

| Функция | Eff | Видимость | Эффекты |
|---|---:|---|---|
| `run` | 3 | export | `Net Io Fail[StartupError]` |
| `route` | 9 | private (выводится) | `Db Cache Logger Clock IdGen Metrics Trace Idempotency Fail[HttpError]` |
| `run_in_test_mode` | 1 | private | `Fail` |

## Сводная статистика

23 функции, из них 10 `export`, 13 private.

| Метрика | Все | Только public (`export`) | Только private |
|---|---:|---:|---:|
| Среднее эффектов | 2.7 | 4.2 | 1.6 |
| Медиана | 1 | 3 | 1 |
| Максимум | 10 | 10 | 9 |
| Функций с ≥5 эффектов | 3 | 2 | 1 |
| Функций с 0 эффектов (чистых) | 5 | 0 | 5 |

## Главные выводы

### 1. «Десятки эффектов в каждой функции» — НЕ подтверждается

Максимум — **10 эффектов** (`transfer_money`, `handle_create_transfer`).
Это не «десятки», а чёткий потолок, обусловленный набором cross-cutting
concerns этого сервиса (Db, Cache, Logger, Clock, IdGen, AuthContext,
Metrics, Trace, Idempotency + Fail). У типового backend'а их и
бывает 8–12, столько же ThreadLocal'ов и `@Autowired`-полей в
эквивалентном Spring-сервисе.

Медиана **— 1 эффект**. Половина функций трогает один эффект (обычно
`Fail`) или ноль.

### 2. Плотность сильно различается по слоям

```
chart: эффектов на функцию по слою
────────────────────────────────────────────────
chart  слой        медиана  макс
domain (типы)        —        —     (там нет функций)
repository           3        3
service              1       10
http                 1       10
main / wiring        3        9
────────────────────────────────────────────────
```

- **Repository однообразен**: всегда `Db [+ Cache] + Fail`. 2–3 эффекта.
- **Service бимодален**: одна жирная public-функция (10) + куча
  чистых валидаторов (1).
- **Helper'ы и валидаторы — 0–1 эффект.** Их много, и они «разбавляют»
  средние цифры в пользу читаемости.
- **`run` имеет всего 3 эффекта** (`Net Io Fail`) — все инфраструктурные
  handler'ы привязаны через `with` и не «протекают» в сигнатуру.

### 3. Pattern: жирные сигнатуры — это «семейство дублей»

В примере **две** функции с 10 эффектами — но это одна и та же
сигнатура (handler ровно делегирует service'у). 9-эффектная `route`
имеет тот же набор минус `AuthContext` (он добавляется ВНУТРИ через
`with`). 7-эффектный `do_transfer` — это `transfer_money` минус 3
эффекта, которые public-функция «съела» через `with`.

Иначе говоря: жирная сигнатура — это **инвентарь cross-cutting concerns
сервиса**. Она появляется в горлышке (entry point), повторяется на
1–2 уровнях рядом, и быстро тает по мере того, как handler'ы
закрываются `with`-блоками.

### 4. Private-функции пишут БЕЗ эффектов в сигнатуре (D28)

В файле `do_transfer` записан как:

```nova
fn do_transfer(req TransferRequest) -> Transfer { ... }
```

Без эффектов. Компилятор выводит `Db Cache Logger Clock IdGen Metrics
Fail[TransferError]` (7 штук) и показывает их через
`nova check --show-effects`. То есть **читаемость кода** в private
не страдает от плотности — сигнатура голая. Расплата за это —
дисциплина тулинга и ошибка компиляции на public-границе, если private
случайно «приобрёл» лишний эффект.

### 5. Чистых функций — много, и они помечены КАК чистые

5 из 23 функций (22%) — БЕЗ эффектов в сигнатуре. Это валидаторы,
конвертеры, мапперы. Компилятор гарантирует их детерминизм — их можно
мемоизировать, вызывать в любом потоке, использовать в hot loop
без yield-point'ов.

В Java/Python эта информация недоступна без аудита тела функции:
любая «утилита форматирования» теоретически может вызвать `Logger.info`
или дёрнуть БД через DI-инжекшен.

## Сравнение с эквивалентным Spring-сервисом

Возьмём `transfer_money` (10 эффектов в Nova) и сделаем эквивалент
на Spring Boot:

```java
@Service
public class TransferService {
    @Autowired private AccountRepository accountRepo;
    @Autowired private TransferRepository transferRepo;
    @Autowired private CacheManager cache;
    @Autowired private Logger logger;
    @Autowired private MeterRegistry metrics;
    @Autowired private Tracer tracer;
    @Autowired private IdempotencyService idem;
    @Autowired private AuthService auth;
    @Autowired private Clock clock;

    @Transactional
    public Transfer transferMoney(TransferRequest req)
        throws TransferException
    {
        // ... та же логика
    }
}
```

Сигнатура **выглядит** короче — всего `throws TransferException`. Но:

- 9 `@Autowired`-полей — тех же 9 эффектов, только невидимых в типе.
- `@Transactional` — это `Db.in_transaction(...)`, аннотация-магия.
- `Tracer` обычно через `MDC` — невидимый ThreadLocal.
- `Clock` иногда static `System.currentTimeMillis` — не подменяется в тестах вовсе.

**Сложность не исчезла — она спрятана.** Преимущество Nova: AI и
человек видят все 10 побочек в типе функции. Цена — список длиннее.

## Главный механизм управления плотностью — `with`

`with EffectName = handler { body }` **снимает** эффект из сигнатуры
внутри тела:

```
run            (Net Io Fail)
└─ with Db = pg, Cache = redis, ... {
     serve(...)        ← этим эффектам с этого места уже НЕ нужно быть в типе
   }
```

Поэтому пирамида сужается к main:

```
transfer_money     10 ─┐
do_transfer         7  │ ← сюда не доходят AuthContext, Idempotency, Trace
route               9  │ ← сюда добавляется AuthContext (через with в route)
run                 3 ─┘ ← здесь только Net Io Fail
                   max─┘
```

## Что можно ещё уменьшить

В этом примере НЕ применены оптимизации, которые мог бы применить
автор для своего стиля:

1. **Group-эффекты.** Если три эффекта `Logger Metrics Trace` ходят
   парой, можно объявить `effect Observability { ... }` и сократить
   три эффекта до одного. Цена — теряется детализация, что именно
   функция использует.
2. **Erase в местах, где детализация не критична.** Для подсистем,
   где гомогенный набор задач (queue worker'ы), `erase[E]` стирает
   эффекты в один тип. Менее точный контроль, но компактнее.
3. **Псевдоним эффект-группы.** Q21 в `spec/open-questions.md` —
   парковка решения о type alias'ах для наборов эффектов.

Сейчас пример демонстрирует «сырую» картину без агрегации.

## Ответ на исходный вопрос

> «Боюсь, что будет десятки эффектов в каждой функции»

**Нет, не будет.** В реальном backend с 9 cross-cutting concerns
(Db, Cache, Logger, Clock, IdGen, AuthContext, Metrics, Trace,
Idempotency):

- Жирные сигнатуры — у `export fn` бизнес-логики и handler'ов:
  **до 10 эффектов**, и это потолок (равен числу concern'ов сервиса).
- Их в проекте **единицы** — это горлышки, не норма.
- Private-функции эффектов в сигнатуре не пишут — компилятор выводит.
- Утилиты, валидаторы, мапперы — **0–1 эффект**, как обычно.

Беспокойство понятное: глядя на `transfer_money` с 10 эффектами,
кажется, что это «много». Но это РОВНО та сложность, которая в
Java/Go/Python размазана по DI, ThreadLocal'ам и недокументированным
вызовам, и не видна в типе.

Альтернатива «в каждой функции немного» — это «сложность спрятана».
Альтернатива Nova — «сложность видима в одной точке (entry point) и
тает к main и к листьям». Это не больше эффектов, это более честное
их распределение.
