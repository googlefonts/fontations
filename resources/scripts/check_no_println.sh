#!/usr/bin/env bash

# Script to check for println! or eprintln! in Rust files, ignoring comments.
#
# When no args are provided, all the git-tracked *.rs files are searched,
# except those listed in the 'println_ignore.txt' file next to the script.
# When run with two args, respectively --from-ref <ref> and --to-ref <ref>,
# only the added lines from the git diff between the two refs are searched.
# The latter is useful when running this as a pre-push git hook.

SCRIPTS_DIR=$(dirname "$(realpath "$0")")
PRINTLN_IGNORE_LIST="$SCRIPTS_DIR/println_ignore.txt"


grep_rust_lines_with_prints() {
    for file in $(git ls-files '*.rs'); do
        # skip files listed in the ignore list
        if ! grep -q -F -x "$file" "$PRINTLN_IGNORE_LIST"; then
            # Strip all inline comments
            sed -e 's://.*$::' "$file" | \
            # Prepend the file name and line number to each line, with colors
            awk -v file="$file" '{ printf "\033[35m%s\033[0m:\033[32m%s\033[0m: %s\n", file, FNR, $0 }' | \
            # Grep for print statements
            grep --color=always -E '\b(println!|eprintln!)' || true
        fi
    done
}

diff_rust_lines_with_prints() {
    local from_ref="$1"
    local to_ref="${2:-HEAD}"
    # --diff-filter=A is to list only added files, excluding modified/deleted
    git diff "$from_ref" "$to_ref" --name-only --diff-filter=A | \
    grep '\.rs$' | \
    while read -r file; do
        # skip files listed in the ignore list
        if ! grep -q -F -x "$file" "$PRINTLN_IGNORE_LIST"; then
            # For each file, get a unified diff with no context (-U0)
            git diff "$from_ref" "$to_ref" -U0 -- "$file" | \
            # Strip lines starting with '+++' containing the file name
            grep -v -E '^\+\+\+' | \
            # Select only the added lines, i.e. starting with a plus sign
            grep -E '^\+' | \
            # Strip the '+' prefix and the inline comments
            sed -e 's/^\+//' -e 's://.*$::' | \
            # Prepend the file name and line number to each line, with colors
            awk -v file="$file" '{ printf "\033[35m%s\033[0m:\033[32m%s\033[0m: %s\n", file, FNR, $0 }' | \
            # Grep for print statements
            grep --color=always -E '\b(println!|eprintln!)' || true
        fi
    done
}


opt_error() {
    local message="$1"
    echo "Error: $message"
    echo "Usage: $0 [--from-ref <ref>] [--to-ref <ref>]"
    exit 1
}


if [ $# -eq 0 ]; then
    # when no arguments are passed, search all git-tracked files
    matches=$(grep_rust_lines_with_prints)
else
    from_ref=""
    to_ref=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --from-ref)
                from_ref="$2"
                shift 2
                ;;
            --to-ref)
                to_ref="${2:-HEAD}"
                shift 2
                ;;
            *)
                opt_error "Unknown option: $1"
                ;;
        esac
    done

    if [ -z "$from_ref" ] || [ -z "$to_ref" ]; then
        opt_error "both --from-ref and --to-ref must be provided, or none at all."
    fi

    matches=$(diff_rust_lines_with_prints "$from_ref" "$to_ref")
fi

if [ ! -z "$matches" ]; then
    echo "Error: The following Rust source files contain println! or eprintln! statements:"
    echo "$matches"
    echo "Please remove or comment out the println! and eprintln! statements before pushing."
    echo "You may also add the files to 'println_ignore.txt' if you want to keep as is."
    exit 1
fi
exit 0
