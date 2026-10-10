Reference generated with HarfBuzz a3845e8d1e on the synthetic avar2
Roboto Flex font constructed by `instance::avar2::tests::truetype_font(true)`.
The weight row uses deltas `[1024, 512, -512]` over one weight region and
two identical width regions. The varying width contributions cancel,
allowing the weight pin to be removed. The width and optical-size rows
retain nonzero deltas, exercising row and axis renumbering.

Command: `hb-subset --gids='*' --layout-features='*' --name-IDs='*'
--name-languages='*' --notdef-outline --drop-tables= --instance=wght=700`.

Only outline, metrics, variation, and layout tables are stored; naming,
cmap, post, STAT, and hint-program tables are omitted.

`dependent-pruned.ttf` was generated with the same command using HarfBuzz
0bd344a29 and `truetype_font(false)`, whose weight row uses
`[1024, 512, -256]`. The dependent weight pin remains hidden. Unreachable
regions are removed, reducing gvar from 226 tuples to 149; surviving
tuple headers, point numbers, and packed deltas retain their meaning.
