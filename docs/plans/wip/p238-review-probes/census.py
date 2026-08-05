import re, sys, os, glob, json

def find_files(root):
    out = []
    for dirpath, dirnames, filenames in os.walk(root):
        # skip build artifact dirs
        parts = dirpath.replace("\\", "/").split("/")
        if any(p in (".pbuild", "target", ".git") for p in parts):
            continue
        for fn in filenames:
            if fn.endswith(".nv") and not fn.endswith("_test.nv"):
                out.append(os.path.join(dirpath, fn))
    return out

FN_DECL_RE = re.compile(r'^\s*(export\s+)?(extern\s+("nova"|"c")\s+)?fn\s+', re.MULTILINE)

def extract_functions(text):
    """Yield (decl_line, sig_text, body_text_or_None) for each top-level fn decl."""
    n = len(text)
    matches = list(FN_DECL_RE.finditer(text))
    results = []
    for i, m in enumerate(matches):
        start = m.start()
        # signature ends at first top-level '{' (block body) or '=>' (arrow body)
        # not nested inside (), [] from params/generics.
        depth_paren = 0
        depth_brack = 0
        j = m.end()
        sig_end = None
        body_kind = None
        in_str = False
        str_ch = ''
        k = j
        limit = matches[i+1].start() if i+1 < len(matches) else n
        while k < limit:
            c = text[k]
            if in_str:
                if c == '\\':
                    k += 2
                    continue
                if c == str_ch:
                    in_str = False
                k += 1
                continue
            if c == '"' or c == "'":
                in_str = True
                str_ch = c
                k += 1
                continue
            if c == '(':
                depth_paren += 1
            elif c == ')':
                depth_paren -= 1
            elif c == '[':
                depth_brack += 1
            elif c == ']':
                depth_brack -= 1
            elif c == '{' and depth_paren <= 0 and depth_brack <= 0:
                sig_end = k
                body_kind = 'block'
                break
            elif c == '=' and depth_paren <= 0 and depth_brack <= 0 and text[k:k+2] == '=>':
                sig_end = k
                body_kind = 'arrow'
                break
            k += 1
        if sig_end is None:
            # signature-only (protocol method decl, no body) — record with no body
            results.append((m.start(), text[m.start():limit], None))
            continue
        sig_text = text[m.start():sig_end]
        if body_kind == 'block':
            # brace match from sig_end
            depth = 0
            p = sig_end
            in_str2 = False
            str_ch2 = ''
            body_start = sig_end
            body_end = None
            while p < n:
                c = text[p]
                if in_str2:
                    if c == '\\':
                        p += 2
                        continue
                    if c == str_ch2:
                        in_str2 = False
                    p += 1
                    continue
                if c == '"' or c == "'":
                    in_str2 = True
                    str_ch2 = c
                    p += 1
                    continue
                if c == '{':
                    depth += 1
                elif c == '}':
                    depth -= 1
                    if depth == 0:
                        body_end = p
                        break
                p += 1
            body_text = text[body_start:body_end+1] if body_end else text[body_start:min(limit, n)]
        else:
            # arrow body: until next top-level statement terminator — approximate
            # as rest up to `limit` (next fn decl) since Nova doesn't use ';'.
            body_text = text[sig_end:limit]
        results.append((m.start(), sig_text, body_text))
    return results

def classify(sig_text, body_text):
    has_body = body_text is not None
    is_extern = bool(re.search(r'\bextern\b', sig_text))
    # generic type params: fn name[ ... ]( ... )  OR receiver Type[ ... ] @method
    is_generic = bool(re.search(r'fn\s+[\w.]*\[[^\]]*\]', sig_text)) or bool(re.search(r'fn\s+\w+\[[^\]]+\]\s*(@|\.)', sig_text))
    has_fn_param = bool(re.search(r'[\(,]\s*\w*\s*fn\s*\(', sig_text))
    body = body_text or ""
    has_mut = bool(re.search(r'\bmut\b', sig_text)) or bool(re.search(r'\bmut\b', body))
    has_lock = bool(re.search(r'\.lock\(\)', body))
    has_spawn = bool(re.search(r'\bspawn\b|\bdetach\b|\bparallel\s+for\b|\bsupervised\b', body))
    # Tighter proxy: field write via @-sigil (self/receiver mutation reaching
    # outside the function's own locals) — excludes pure local-`mut` idiom
    # (accumulators, with-star builder locals never escaping).
    has_field_write = bool(re.search(r'@\w+\s*=(?!=)', body))
    # mut LOCAL only: has_mut but no field write and receiver/self not itself
    # reassigned structurally — rough "probably local-only mut" flag.
    mut_local_only = has_mut and not has_field_write
    return dict(has_body=has_body, is_extern=is_extern, is_generic=is_generic,
                has_fn_param=has_fn_param, has_mut=has_mut, has_lock=has_lock, has_spawn=has_spawn,
                has_field_write=has_field_write, mut_local_only=mut_local_only)

def run(roots, label):
    files = []
    for r in roots:
        files += find_files(r)
    total = 0
    no_body = 0
    stats = dict(trivially_safe=0, via_lock=0, direct_mut_no_lock=0,
                 cant_decide_generic=0, cant_decide_extern=0, cant_decide_fnparam=0,
                 cant_decide_any=0, has_spawn=0,
                 direct_mut_field_write_no_lock=0, direct_mut_local_only_no_lock=0)
    examples = dict(direct_mut_no_lock=[], cant_decide_fnparam=[])
    for f in files:
        try:
            text = open(f, encoding='utf-8').read()
        except Exception as e:
            continue
        for (pos, sig, body) in extract_functions(text):
            total += 1
            if body is None:
                no_body += 1
                continue
            c = classify(sig, body)
            cant_decide = c['is_generic'] or c['is_extern'] or c['has_fn_param']
            if cant_decide:
                stats['cant_decide_any'] += 1
                if c['is_generic']: stats['cant_decide_generic'] += 1
                if c['is_extern']: stats['cant_decide_extern'] += 1
                if c['has_fn_param']:
                    stats['cant_decide_fnparam'] += 1
                    if len(examples['cant_decide_fnparam']) < 8:
                        examples['cant_decide_fnparam'].append(f + ':' + sig.strip().split('\n')[0][:100])
            elif c['has_lock']:
                stats['via_lock'] += 1
            elif c['has_mut']:
                stats['direct_mut_no_lock'] += 1
                if c['has_field_write']:
                    stats['direct_mut_field_write_no_lock'] += 1
                else:
                    stats['direct_mut_local_only_no_lock'] += 1
                if len(examples['direct_mut_no_lock']) < 8:
                    examples['direct_mut_no_lock'].append(f + ':' + sig.strip().split('\n')[0][:100])
            else:
                stats['trivially_safe'] += 1
            if c['has_spawn']:
                stats['has_spawn'] += 1
    print(f"=== {label} ===")
    print(f"files: {len(files)}  total fn decls (incl signature-only): {total}  signature-only(no body): {no_body}")
    impl_total = total - no_body
    print(f"implementations (with body): {impl_total}")
    for k, v in stats.items():
        pct = (100.0*v/impl_total) if impl_total else 0
        print(f"  {k}: {v}  ({pct:.1f}%)")
    print("sample direct_mut_no_lock:", examples['direct_mut_no_lock'])
    print("sample cant_decide_fnparam:", examples['cant_decide_fnparam'])
    print()
    return dict(total=total, no_body=no_body, impl_total=impl_total, stats=stats)

if __name__ == '__main__':
    import sys
    root = sys.argv[1]
    label = sys.argv[2]
    run([root], label)
