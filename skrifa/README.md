# skrifa

This is a library for high level loading of glyph outlines (and eventually color outlines and bitmaps)
from font files. The intention is fully featured (e.g. variations and hinting) support for all glyph sources
except for the SVG table.

This is part of the [oxidize](https://github.com/googlefonts/oxidize) project.

## Features

Current (✔️), near term (🔜) and planned (⌛) feature matrix:

| Source | Loading | Variations | Hinting |
|--------|---------|------------|---------|
| glyf   | ✔️     |  ✔️        | ⌛*    |
| CFF    | ⌛     | ⌛         | ⌛     |
| CFF2   | ⌛     | ⌛         | ⌛     |
| COLRv0 | 🔜     | 🔜         | *      |
| COLRv1 | 🔜     | 🔜         | *      |
| EBDT   | 🔜     | -          | -      |
| CBDT   | 🔜     | -          | -      |
| sbix   | 🔜     | -          | -      |

\* A working implementation exists for hinting but is not yet merged.

\*\* This will be supported but is probably not desirable due the general affine transforms
present in the paint graph.

## The name?

Following along with our theme, *skrifa* is Old Norse for "write" or "it is written." And
so it is named.
