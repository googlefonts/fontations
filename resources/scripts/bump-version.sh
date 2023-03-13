#!/bin/bash
# See usage() for usage

source rel-common.sh

# Helpers
function usage() {
  echo "Usage: ./bump-version.sh crate1 crate2 crateN bump"
  echo "       ./bump-version.sh read-fonts write-fonts patch"
  echo "       ./bump-version.sh {read,write}-fonts patch"
  echo "bump is as defined by cargo release: major minor or patch"
}

# What is it you want us to do?
if [ $# -eq 0 ]; then
  die_with_usage "No arguments provided, must specify crate(s)"
fi

# bump is the last argument, crate list is everythign else
bump="${@:$#}"
set -- "${@:1:$(($#-1))}"
crates=("$@")

# Validate
[[ "$bump" =~ ^(major|minor|patch)$ ]] || die_with_usage "Invalid bump, must be major, minor, or patch"

validate_crates "${crates[@]}"

# Do the thing. We set errexit so step failure should break us out.
for crate in "${crates[@]}"
do
  cargo release version "${bump}" -p "${crate}" -x
done

echo "NEXT STEPS"
echo "Commit these changes to a new branch, get it approved and merged, and switch to the up-to-date main."