# fauntlet - the *f*ont g*auntlet*

A tool to compare the output of [Skrifa](https://github.com/googlefonts/fontations/skrifa)
and [FreeType](https://freetype.org).

## Capabilities

This currently compares glyph outlines (in path form) and advance widths for
fixed sets of sizes and locations in variation space. Prior to comparison,
the outlines are "regularized" to account for minor differences in output.
These differences do not affect rendering and exist for the following reasons:

1. All contours are implicitly closed in FreeType. Skrifa emits close
elements to allow our output to be further processed by, e.g. a stroke
converter without requiring modification.
2. The FreeType CFF loader drops degenerate move and line elements but does
so before a final scaling step which may produce additional unused path
commands. Skrifa performs this filtering step after all scaling steps have
been applied, leading to more aggressive removal of degenerates.
3. In unscaled (`FT_LOAD_NO_SCALE`) mode, the FreeType TrueType loader
yields outlines with coordinates in integral font units. Passing these to
`FT_Outline_Decompose` results in truncated coordinates for some implicit
oncurve points (those with odd deltas in either direction). Skrifa produces
fractional values for these midpoints.

## Usage

To compare glyphs for a single font:

```bash
cargo run --release -p fauntlet -- compare-glyphs path/to/font.ttf
```

You can specify multiple font files and each file may use glob syntax. For example:

```bash
cargo run --release -p fauntlet -- compare-glyphs /myfonts/noto/**/hinted/**.ttf otherfonts/*.?tf
```

This will process all fonts in parallel, emitting diagnostics to `stderr` if
mismatches are found. Use the `--exit-on-fail` flag to end the process on the
first failure.
