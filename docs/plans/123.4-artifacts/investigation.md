// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 123.4 Ф.0 — Investigation + DECISIONs A.4-F.4

> **Дата:** 2026-06-02.

---

## 1. AST shape для chain access

`@a.b.c` parses to nested Member:
```
Member {
    obj: Box<Expr<
        Member {
            obj: Box<Expr<
                Member {
                    obj: Box<Expr<SelfAccess>>,
                    name: "a"
                }
            >>,
            name: "b"
        }
    >>,
    name: "c"
}
```

Chain detection: walk path до SelfAccess root, collect path components
`["a", "b", "c"]`.

## 2. Chain path canonicalization

For each chain expression `@a.b.c`, canonical form is `Vec<String>`
path. Match via `extract_chain_path(e) -> Option<Vec<String>>`.

## 3. Sub-path occurrences

For chain `@a.b.c`:
- Sub-path `["a"]` — `@a` direct.
- Sub-path `["a", "b"]` — `@a.b`.
- Sub-path `["a", "b", "c"]` — `@a.b.c`.

Multiple occurrences of `@a.b.c` in body imply repeating prefixes
`@a`, `@a.b`. V4 caches longest prefix accessed ≥ threshold.

For V4 simplicity: cache only longest-path occurrences (terminal
chains). Shorter prefixes handled by D217 baseline.

Example:
- Body has 3 × `@parent.inner.cfg` accesses.
- V4 cache: `_at_parent_inner_cfg_chain = @parent.inner.cfg`.
- D217 separately caches `@parent` if also direct-accessed ≥
  threshold.

## 4. Eligibility (V4 conservative)

- Chain length ≥ 2 (i.e., at least `@a.b`).
- Chain length ≤ 4.
- Count of identical chain occurrences ≥ pure_threshold.
- No closure capture of any chain segment.
- No concurrent body.
- No top-level field write для chain's root field.
- No mutation of intermediate path segments (handled
  conservatively — if ANY @F write exists in body for any F that's
  a path component, skip).

## 5. Composition

Order в cache_module:
1. D218 LICM (hoists @F).
2. **V4 chain cache** (NEW phase).
3. D219 pure-cache.
4. D217 per-fn cache.

V4 emit'ит `ro _at_<a>_<b>_<c>_chain = @<a>.<b>.<c>` at body
prefix; replaces chain expressions с Ident.

## 6. DECISIONs A.4-F.4

- **A.4:** scope = chain length 2-4, ≥ threshold occurrences.
- **B.4:** naming `_at_<a>_<b>_<c>_chain`.
- **C.4:** threshold same as D217 (2). Max-per-fn shared (8).
- **D.4:** composition order LICM → chain → pure → per-fn.
- **E.4:** conservative — any @F write in body skips all chain
  caching for that root.
- **F.4:** closure-in-body / concurrent / protocol receiver skip.

## 7. Closure

DECISIONs A.4-F.4 finalized. Implementation simple: walk body,
detect chains, count by canonical path, emit caches.
