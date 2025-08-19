#!/bin/bash
# See usage() for usage

source "$(dirname -- "$0";)/rel-common.sh"

# Helpers
function usage() {
  echo "Usage:"
  echo "       # release everything changed"
  echo "       ./release.sh"
  echo
  echo "       # release one crate"
  echo "       ./release.sh write-fonts"
  echo
  echo "Typically you should be running this after bump-version.sh"
}

handle_publish_interrupt() {
    echo
    echo "Publish interrupted by CTRL-C."
    echo -n "Do you want to continue with tagging? [auto-continue in 5 seconds] (Y/n): "
    if read -t 5 -r response; then
        if [[ "$response" =~ ^[Nn]$ ]]; then
            echo "Stopping release process."
            exit 130  # Exit with SIGINT code
        fi
    else
        # No input means 'Yes'; add newline after timeout
        echo
    fi
}

# What is it you want us to do?
if [ $# -gt 1 ]; then
  die_with_usage "Specify 0 - meaning all - or 1 packages"
fi
crate_specifier=""
if [ $# -eq 1 ]; then
  crates=("$@")
  validate_crates "${crates[@]}"
  crate_specifier="-p ${crates[0]}"
fi

# Do the thing. We set errexit (in rel-common.sh) so step failure should break us out.

echo "Dry run..."
cargo release publish ${crate_specifier}

echo "Doing the thing; ${COLOR_RED}PRESS CTRL+C if anything looks suspicious${TEXT_RESET}"

echo "Publish to crates.io"

# Set up a trap to handle CTRL-C during cargo release publish
trap 'handle_publish_interrupt' SIGINT
cargo release publish -x ${crate_specifier}
trap - SIGINT  # Remove trap

echo "Generate tags"
cargo release tag -x ${crate_specifier}  # this prompts y/N
echo "Pushing tag to github"
git push --tags

echo "NEXT STEPS"
echo "You probably want to create a release on github"
