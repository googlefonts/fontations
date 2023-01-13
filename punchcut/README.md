# punchcut

This is a library for high level loading of glyph outlines (and eventually color outlines and bitmaps)
from font files. The intention is fully featured (e.g. variations and hinting) support for all glyph sources
except for the SVG table.

This is part of the [oxidize](https://github.com/googlefonts/oxidize) project.

## Features

Current (✔️), near term (🔜) and planned (⌛) feature matrices:

#### Vector sources:

| Source | Loading | Variations | Hinting |
|--------|---------|------------|---------|
| glyf   | ✔️     |  🔜        | ⌛*    |
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

Wikipedia says "[punchcutting](https://en.wikipedia.org/wiki/Punchcutting) is a craft used in traditional
typography to cut letter punches in steel as the first stage of making metal type." The punches carry the
outline of the desired letter which can be used to create a mold to transfer the design to various
surfaces.

The primary purpose of this crate is the generation of outlines from font data, so the name seemed
appropriate.
