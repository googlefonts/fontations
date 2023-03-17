set -o nounset
set -o errexit

# Colors courtesy of https://stackoverflow.com/a/20983251
TEXT_RESET=$(tput sgr0)
COLOR_RED=$(tput setaf 1)

function die_with_usage() {
    >&2 echo "${COLOR_RED}${1}${TEXT_RESET}"
    usage
    exit 1
}

function validate_crates() {
    for crate in "$@"
    do
      [ -d "$crate" ]|| die_with_usage "Invalid crate: ${crate}"
    done
}