# Ядро check-effect-boundary-shape.sh — ОДИН проход, без процесса на строку.
#
# Первая редакция гоняла цикл оболочки с `grep` на каждую операцию: шесть правил
# на ~120 операций = ~700 процессов, и страж не уложился в две минуты. Страж,
# тормозящий гейт, будет отключён (реестр №475) — поэтому разбор здесь целиком
# в awk, а оболочка только собирает список файлов.
#
# Вход: .nv-файлы. Выход: строки "RN описание" по одному нарушению.

/^[[:space:]]*(export[[:space:]]+)?type[[:space:]]+[A-Za-z_][A-Za-z0-9_]*(\[[^]]*\])?[[:space:]]+effect([[:space:]]|\{)/ {
    eff = $0
    sub(/^[[:space:]]*(export[[:space:]]+)?type[[:space:]]+/, "", eff)
    sub(/[[:space:]]+effect.*/, "", eff)
    sub(/\[.*/, "", eff)
    inside = 1
    depth = 0
}

inside {
    line = $0
    sub(/\/\/.*/, "", line)
    d = gsub(/\{/, "{", line) - gsub(/\}/, "}", line)
    depth += d

    gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)

    if (line != "" && line !~ /^(export[[:space:]]+)?type/ && index(line, "(") > 0) {
        name = line
        sub(/\(.*/, "", name)
        gsub(/[[:space:]]/, "", name)

        args = line
        sub(/^[^(]*\(/, "", args)
        sub(/\).*/, "", args)

        # R1 — сырой указатель в подписи (D456 §5).
        if (line ~ /\*\(/ || line ~ /\*mut[[:space:]]/ || line ~ /\*const[[:space:]]/)
            print "R1 сырой указатель: " eff " — " name

        # R3 — счётчик рядом с данными (D456 §6).
        if (args ~ /\[\][A-Za-z_][A-Za-z0-9_]*[^,]*,[[:space:]]*[A-Za-z_][A-Za-z0-9_]*(c|_count|_len)[[:space:]]+int/)
            print "R3 счётчик рядом с данными: " eff " — " name

        # R4 — позиционная простыня (D456 §8).
        if (args != "") {
            n = split(args, _p, ",")
            if (n > 6) print "R4 позиционная простыня (" n " параметров): " eff " — " name
        }

        # R5 — out-параметр (D456 §4).
        if (args ~ /(^|,)[[:space:]]*mut[[:space:]]+out/)
            print "R5 out-параметр: " eff " — " name

        # R6 — сырая ручка как int (D456 §5). Ловим по ИМЕНИ параметра: сам по
        # себе `int` законен (`port`, `mode`, `offset`); незаконно им обозначать
        # РЕСУРС.
        if (args ~ /(^|,)[[:space:]]*(fd|h|handle|sock|socket|stream|listener)[[:space:]]+int([[:space:]]|,|$)/)
            print "R6 сырая ручка как int: " eff " — " name

        # R2 — копим имена для пары «сколько» + «дай i-й» (D456 §3).
        if (name ~ /(_count|_len)$/) has_n[eff] = 1
        if (name ~ /_at$/)           has_at[eff] = 1
        seen[eff] = 1
    }

    if (depth <= 0 && index($0, "}") > 0) inside = 0
}

END {
    for (e in seen)
        if (has_n[e] && has_at[e])
            print "R2 обход по индексу: " e " — есть и *_count/*_len, и *_at"
}
