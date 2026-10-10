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
