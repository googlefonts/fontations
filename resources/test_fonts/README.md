# test files

This directory contains files used for testing. The masters are the ttx files;
these are human readable/editable. From these, we generate the binary ttf
files that are the actual test inputs.

## rebuilding
To update the binaries, run the `./resources/test_fonts/rebuild.sh` from the
repo root. This script will install the correct version of fonttools, and then
regenerate all of the inputs.

```shell
# from the repo root
somewhere/fontations $ ./resources/test_fonts/rebuild.sh
```
