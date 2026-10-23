# test files

This directory contains files used for testing.


## test font sources
Describes the provenance, usage and generation procedures for font data used for testing.

  * source: https://github.com/harfbuzz/harfbuzz/tree/main/test/subset/data/fonts
  * source license file: https://github.com/harfbuzz/harfbuzz/blob/main/test/COPYING
  * license: [Open Font License][OFL]
  * usage: subsetter testing
    ```shell
    cargo run -- --path=font-file --text=abc --output-file=subset.ttf
    ```

[OFL]: https://scripts.sil.org/cms/scripts/page.php?site_id=nrsi&id=OFL

The `tt-instance-*.ttf` references are full instances of the existing fixtures,
generated with the local HarfBuzz subsetter using `--gids=* --notdef-outline
--name-IDs=*`. Requests and source fonts are listed in
`tests/truetype_instance_test.rs`; these references retain the source licenses.

The `tt-partial-*.ttf` references use the same HarfBuzz command with pin and
range requests listed in `tests/truetype_instance_test.rs`. Tests compare the
default outlines, axis records, and second-stage instances at retained locations.

The `expected/varc` references are subsets of the existing VARC fonts in
`font-test-data/test_data/ttf`, generated with the local HarfBuzz subsetter using
`--notdef-outline --name-IDs=*` and the glyph selections and retain-GID modes
listed in `tests/varc_test.rs`. They retain the source fixtures' licenses.
`fonts/tt-zero-contour-instructions.ttf` exercises instructions for phantom
points and a varying advance without outline points. Regenerate it with
`fonts/generate-tt-zero-contour-instructions.py` using FontTools with support
for preserving zero-contour instructions.

`fonts/tt-tuple-precision.ttf` covers float rounding boundaries in tuple scalars,
residual IUP interpolation, and rebased tent coordinates. Regenerate it with
`fonts/generate-tt-tuple-precision.py`; the assertions in
`tests/truetype_instance_test.rs` use independently checked HarfBuzz results.
