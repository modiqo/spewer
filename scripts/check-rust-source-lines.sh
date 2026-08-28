#!/bin/sh
set -eu

limit=500

find src tests -type f -name '*.rs' -print | sort | {
    failed=0
    while IFS= read -r file; do
        lines=$(wc -l < "$file")
        if [ "$lines" -gt "$limit" ]; then
            printf '%s: %s lines exceeds the %s-line limit\n' \
                "$file" "$lines" "$limit" >&2
            failed=1
        fi
    done

    if [ "$failed" -eq 0 ]; then
        printf 'Rust source line limit: ok\n'
    fi
    exit "$failed"
}

