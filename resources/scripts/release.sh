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
# Temporarily disable errexit so we can catch a CTRL-C at the end of `publish`
# from the impatient who doesn't want to wait for index changes to propagate
set +e
cargo release publish -x ${crate_specifier}
publish_result=$?
set -e  # Re-enable errexit

if [ $publish_result -eq 130 ]; then  # exit code for SIGINT (CTRL-C)
    echo "Publish interrupted by CTRL-C."
    echo -n "Do you want to continue with tagging? [auto-continue in 5 seconds] (Y/n): "
    if read -t 5 -r response; then
        if [[ "$response" =~ ^[Nn]$ ]]; then
            echo "Stopping release process"
            exit $publish_result
        fi
    else
        echo  # newline after timeout
    fi
elif [ $publish_result -ne 0 ]; then
    exit $publish_result
fi

echo "Generate tags"
cargo release tag -x ${crate_specifier}  # this prompts y/N
echo "Pushing tag to github"
git push --tags

echo "NEXT STEPS"
echo "You probably want to create a release on github"
