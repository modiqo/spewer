#!/bin/sh
set -eu

limit=500

find . -path './target' -prune -o -path './generated' -prune -o \
    -type f -name '*.md' -print | sort | {
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
        printf 'Markdown line limit: ok\n'
    fi
    exit "$failed"
}

