# test files

This directory contains files used for testing. The masters are the ttx files;
these are human readable/editable. From these, we generate the binary ttf
files that are the actual test inputs.

## rebuilding
 To update the binaries, run the `./resources/test_fonts/rebuild.sh` from the repo root.

```shell
# from the repo root
somewhere/fontations $ ./resources/test_fonts/rebuild.sh
```

## fonttools/ttx

You will need to have fonttools installed to update these scripts. We record the
version last used to regenerate the test inputs in the
`resources/test_fonts/ttf/GENERATED_BY_TTX_VERSION` file.

