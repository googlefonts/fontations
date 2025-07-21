# Collection of fonts for testing against FreeType in CI

This is a hand curated selection of fonts to test Skrifa output against
FreeType on each pull request to prevent obvious regressions.

## Running tests

```shell
cargo run --release -p fauntlet -- compare-glyphs --hinting-engine all --hinting-target all --exit-on-fail test_fonts/*.*
```

## Fonts

* Cantarell ([license][OFL])): Variable CFF2 font with hinting
* Material Icons ([license][Apache2]): Tricky CFF font that revealed
  a prior [bug](https://github.com/googlefonts/fontations/issues/1184)
* Noto Sans Hebrew ([license][OFL]): Hinted (ttfautohint) TrueType font that also
  exercises extra features of the runtime autohinter
* Noto Sans Gurmukhi ([license][OFL]): Unhinted variable TrueType font
  that revealed a prior [bug](https://github.com/googlefonts/fontations/issues/1500)

[OFL]: https://scripts.sil.org/cms/scripts/page.php?site_id=nrsi&id=OFL
[Apache2]: https://www.apache.org/licenses/LICENSE-2.0
