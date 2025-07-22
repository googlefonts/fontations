# Collection of fonts for testing against FreeType in CI

This is a hand curated selection of fonts to test Skrifa output against
FreeType on each pull request to prevent obvious regressions.

## Running tests

```shell
cargo run --release -p fauntlet -- compare-glyphs --hinting-engine all --hinting-target all --exit-on-fail test_fonts/*.*
```

## Fonts

* Cantarell-VF.subset.otf
  * font: Cantarell
  * license: ([Open Font License][OFL]))
  * source: https://fonts.google.com/specimen/Cantarell
  * usage: Variable CFF2 font with hinting
  * subset: `pyftsubset Cantarell-VF.otf --gids=0-32,630`

* MaterialIcons.subset.otf
  * font: Google Material Icons
  * license: [Apache 2][Apache2]
  * source: https://fonts.googleapis.com/icon?family=Material+Icons
  * usage: Tricky CFF font that revealed a prior [bug](https://github.com/googlefonts/fontations/issues/1184)
  * subset: `pyftsubset MaterialIcons.otf --gids=0-32,78`

* NotoSansHebrew-Regular.ttf
  * font: Noto Sans Hebrew Regular
  * license: [OpenFontLicense][OFL]
  * source: https://fonts.google.com/noto/specimen/Noto+Sans+Hebrew
  * usage: Hinted (ttfautohint) TrueType font that also exercises extra features of the runtime autohinter

* NotoSansGurmukhi[wdth,wght].subset.ttf
  * font: Noto Sans Gurmukhi
  * license: [Open Font License][OFL])
  * source: https://fonts.google.com/noto/specimen/Noto+Sans+Gurmukhi
  * usage: Unhinted variable TrueType font that revealed a prior [bug](https://github.com/googlefonts/fontations/issues/1500)
  * subset: `pyftsubset NotoSansGurmukhi[wdth,wght].ttf --gids=0-32` 

[OFL]: https://scripts.sil.org/cms/scripts/page.php?site_id=nrsi&id=OFL
[Apache2]: https://www.apache.org/licenses/LICENSE-2.0
