//! №129 (реестр 221.1, №125-корень / №149-блокер): честная диагностика для
//! `mono_method_decls` — единственный `HashMap<(String, String), FnDecl>` в
//! `emit_c.rs`'s `CEmitter`, ключ = (receiver-spelling, method-name), одно
//! `FnDecl` на ключ.
//!
//! Для РЕАЛЬНОГО generic-типа (`Vec`, пользовательский `Container[T]`, …) этот
//! ключ уникален на исходную декларацию — чекер (`types/mod.rs`, обработка
//! `Item::Fn`, ~строка 1331) уже отклоняет буквально дублирующую (та же
//! arity+типы-параметров+return-type) декларацию как `E_METHOD_REDEFINITION`
//! / «duplicate definition ... with same signature».
//!
//! Для BLANKET-декларации (`fn[I Proto[T]] I @m(...)`, D355) `recv_type_name`
//! — это ГОЛОЕ ИМЯ типопараметра-получателя («I»), НЕ реальный тип.
//! `check_blanket_conflict` (types/mod.rs, D355 §5) ловит конфликт только
//! ДВУХ blanket'ов на ОДИН и тот же протокол (группирует по имени протокола,
//! сознательно НЕ по буквенному имени typevar — см. её doc-comment). Два
//! blanket'а на РАЗНЫЕ протоколы, использующие ОДНУ и ту же идиоматичную
//! букву («I», «T», …) и ОДНО имя метода, протокольно не конфликтуют — и,
//! если сигнатуры при этом различаются (арность / мутабельность получателя /
//! тип возврата — все три валидные D84-overload оси, types/mod.rs ~1353-1376),
//! чекер держит ОБА объявления как отдельные записи в `env.fns`. У
//! `mono_method_decls` такой оси нет: она хранит РОВНО ОДНО `FnDecl` на ключ,
//! так что вторая регистрация тут молча вытесняла бы первую (last-wins) — и
//! диспетч для ЛЮБОГО из двух получателей ошибочно исполнял бы тело второго.
//!
//! Эмпирически (аудит №129, `scratch_repro/p129_repro*.nv` в
//! `nova-p129`-worktree): без вызова конфликтующего метода сегодня программа
//! собирается ЧИСТО, без единой диагностики (последняя декларация тихо
//! победила); с вызовом — падает во ВНУТРЕННЮЮ ошибку компилятора
//! (`P67-LEGACY`, отдельный, вне периметра №129, пробел резолва return-type
//! чекером для multi-candidate blanket'ов) ДО того, как этот реестр вообще
//! читается для диспетча. Ни то, ни другое — не честная диагностика.
//!
//! Диагностируем ЗДЕСЬ, в момент регистрации (форвард-декларация), — фиксируя
//! коллизию независимо от того, вызывается ли неоднозначный метод вообще, до
//! того как любой из двух плохих исходов может произойти. Повторная вставка
//! ТОГО ЖЕ физического объявления (тот же `Span`) — idempotent, не ошибка.

use crate::ast::FnDecl;
use crate::diag::Span;

/// `Ok(())` — можно вставлять (новый ключ, либо повторная регистрация той же
/// декларации). `Err(..)` — коллизия РАЗНЫХ деклов на одном ключе; текст
/// диагностики цитирует оба span'а и объясняет фикс.
pub(crate) fn check_mono_method_decl_collision(
    existing: Option<&FnDecl>,
    new_span: Span,
    recv_type_name: &str,
    method_name: &str,
) -> Result<(), String> {
    let Some(existing) = existing else { return Ok(()); };
    if existing.span == new_span {
        return Ok(()); // та же физическая декларация — idempotent re-insert.
    }
    Err(format!(
        "[E_MONO_METHOD_KEY_COLLISION] два РАЗНЫХ объявления метода `@{method}()` \
         делят один ключ (получатель=`{recv_type_name}`, метод=`{method_name}`) в реестре \
         generic-методов кодогена (`mono_method_decls`), который хранит РОВНО ОДНО FnDecl на \
         ключ — например, два blanket-объявления (`fn[{recv_type_name} Proto[T]] \
         {recv_type_name} @{method}(...)`), связанных РАЗНЫМИ протоколами, но использующих ОДНУ \
         и ту же букву типопараметра-получателя и одно имя метода (D355 §5 ловит только \
         конфликт в пределах ОДНОГО протокола — разные протоколы под тем же \
         `{recv_type_name}` сюда не попадают), либо валидный по D84 overload (арность / \
         мутабельность получателя / тип возврата — чекер разрешает как отдельные записи в \
         `env.fns`, но здесь для них ровно одно место). Второе объявление молча вытеснило бы \
         первое (last-wins): диспетч по ЛЮБОМУ из двух получателей ошибочно исполнял бы тело \
         ВТОРОГО.\n  первое объявление: {existing_span}\n  второе объявление: {new_span}\n  \
         fix: дай получателю-typevar РАЗНУЮ букву в одной из деклараций (например, `fn[J \
         ДругойProto[T]] J @{method}(...)`), либо переименуй один из методов — реестр кодогена \
         не различает две декларации с одним и тем же (получатель, имя метода), даже если \
         чекер считает их валидными отдельными объявлениями.",
        method = method_name,
        recv_type_name = recv_type_name,
        existing_span = existing.span,
        new_span = new_span,
    ))
}

/// №129 Task C follow-up: `mono_fn_decls` — `HashMap<String, FnDecl>` в
/// `emit_c.rs`, keyed ONLY by bare fn name, storing the body/template for
/// EVERY non-receiver generic free function so the monomorphization worklist
/// (Plan 48) can drain it later. Nova's folder-module package allows two
/// DIFFERENT modules to each declare a module-private generic free fn with
/// the SAME bare name (perfectly legal — nothing outside either module can
/// even reference the other's private symbol by that name); `module.items`
/// at this point is the WHOLE package's flattened item list, so both
/// declarations reach this insertion. Unlike `mono_method_decls`'s blanket
/// case, this is NOT an inherently ambiguous situation — each caller
/// resolves its own module's `helper` unambiguously — but this registry's
/// bare-name key conflates them anyway: whichever declaration is registered
/// SECOND silently overwrites the first, and EVERY caller of either name —
/// regardless of which module's `helper` it meant — gets the same (wrong,
/// for one of the two call sites) monomorphized body. Confirmed empirically
/// (audit №129, `scratch_repro/modtest/`): two peer modules each declaring
/// a private `fn[T] helper(x T) -> int` (bodies returning 111 / 222) —
/// calling module A's OWN `helper` from ITS OWN caller returns 222 (module
/// B's body), a silent wrong-value bug, not a crash.
///
/// The CORRECT fix is a module/file-qualified key threaded through this
/// registry's ~10 read sites (worklist drain, `#realtime`/`#blocking`
/// classification, tuple-arity lookups, …) — out of scope for a same-
/// pattern insert-time check (would need read-site context most of those
/// sites don't currently carry). Diagnosing here (registration time) at
/// least converts the silent wrong-value outcome into an honest, actionable
/// error instead of shipping a miscompile — the proper fix (qualified key)
/// is left to the dedicated follow-up window the case A/C triage calls for.
pub(crate) fn check_mono_fn_decl_collision(
    existing: Option<&FnDecl>,
    new_span: Span,
    fn_name: &str,
) -> Result<(), String> {
    let Some(existing) = existing else { return Ok(()); };
    if existing.span == new_span {
        return Ok(()); // та же физическая декларация — idempotent re-insert.
    }
    Err(format!(
        "[E_MONO_FN_KEY_COLLISION] два РАЗНЫХ объявления generic-свободной функции `{fn_name}` \
         делят один ключ в реестре кодогена (`mono_fn_decls`), который хранит РОВНО ОДНО FnDecl \
         на голое имя функции БЕЗ учёта модуля/файла. Это ЛЕГАЛЬНАЯ ситуация на уровне языка — \
         два РАЗНЫХ модуля вправе каждый объявить свою module-private generic-функцию `{fn_name}` \
         (ничто извне не может сослаться на чужую приватную функцию по этому имени) — но реестр \
         кодогена их путает: вторая регистрация молча вытеснила бы первую (last-wins), и вызовы \
         `{fn_name}(...)` из ЛЮБОГО из двух модулей ошибочно исполняли бы тело ВТОРОЙ.\n  \
         первое объявление: {existing_span}\n  второе объявление: {new_span}\n  \
         fix: переименуй одну из функций (уникальное имя убирает коллизию) — корректный fix \
         реестра (ключ, учитывающий модуль/файл) требует отдельного окна: 129/Task C, см. \
         mono_method_registry.rs.",
        fn_name = fn_name,
        existing_span = existing.span,
        new_span = new_span,
    ))
}
