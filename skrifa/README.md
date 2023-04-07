# skrifa

This crate aims to be a robust, ergonomic, high performance library for reading OpenType fonts. It
is built on top of the [read-fonts](https://github.com/googlefonts/fontations/tree/main/read-fonts) low level
parsing library and is also part of the [oxidize](https://github.com/googlefonts/oxidize) project.

## Features

### Metadata

The following information is currently exposed:

* Global font metrics with variation support (units per em, ascender, descender, etc)
* Glyph metrics with variation support (advance width, left side-bearing, etc)
* Codepoint to nominal glyph identifier mapping
    * Unicode variation sequences
* Localized strings

Future goals include:

* Attributes (stretch, style and weight)
* Variation axes and named instances
    * Conversion from user coordinates to normalized design coordinates
* Color palettes
* Embedded bitmap strikes

### Glyph scaling

Current (✔️), near term (🔜) and planned (⌛) feature matrix:

| Source | Decoding | Variations | Hinting |
|--------|---------|------------|---------|
| glyf   | ✔️     |  ✔️        | ⌛*    |
| CFF    | ⌛     | ⌛         | ⌛     |
| CFF2   | ⌛     | ⌛         | ⌛     |
| COLRv0 | 🔜     | 🔜         | **      |
| COLRv1 | 🔜     | 🔜         | **      |
| EBDT   | 🔜     | -          | -      |
| CBDT   | 🔜     | -          | -      |
| sbix   | 🔜     | -          | -      |

\* A working implementation exists for hinting but is not yet merged.

\*\* This will be supported but is probably not desirable due the general affine transforms
present in the paint graph.

## The name?

Following along with our theme, *skrifa* is Old Norse for "write" or "it is written." And
so it is named.
