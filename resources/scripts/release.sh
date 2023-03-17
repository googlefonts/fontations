#!/bin/bash
# See usage() for usage

source "$(dirname -- "$0";)/rel-common.sh"

# Helpers
function usage() {
  echo "Usage: ./release.sh crate1 crate2 crateN"
  echo "       ./release.sh read-fonts write-fonts"
  echo "       ./release.sh {read,write}-fonts"
  echo "Typically you should be running this after bump-version.sh"
}

# What is it you want us to do?
if [ $# -eq 0 ]; then
  die_with_usage "No arguments provided, must specify crate(s)"
fi

crates=("$@")
validate_crates "${crates[@]}"

# Do the thing. We set errexit so step failure should break us out.

echo "Dry run..."
cargo release publish

echo "Doing the thing; ${COLOR_RED}PRESS CTRL+C if anything looks suspicious${TEXT_RESET}"

echo "Publish to crates.io"
cargo release publish -x  # this prompts y/N
echo "Generate tags"
cargo release tag -x  # this prompts y/N
echo "Pushing tag to github"
git push --tags

echo "NEXT STEPS"
echo "You probably want to create a release on github"
