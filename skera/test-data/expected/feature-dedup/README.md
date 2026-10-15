# Feature deduplication references

`layout-feature-dedup.ttf` derives from the existing Roboto variable composite
fixture, with Unicode A/B/C mapped to glyphs 1/2/3 and synthetic GSUB/GPOS.
Each table has paired same-tag features for the default and English language
systems. Both use lookup 0, but `cv01` changes its character array, `size`
changes its design size, `ss01` changes its UI name, and `ss02` switches to
lookup 1 above normalized weight 0.25. The `ss03` pair is identical and
should still merge. The English records must keep their distinct indices.

References use HarfBuzz 0bd344a29 ([PR #6350](https://github.com/harfbuzz/harfbuzz/pull/6350)), all glyphs,
retained glyph IDs, the .notdef outline, and all layout features. Each case
contains raw GSUB and GPOS tables; compare decoded structures rather than
offset packing. `subset` has no axis request, `partial` pins `wdth=87.5`,
`full-active` pins `wght=650,wdth=87.5`, and `full-default` pins
`wght=400,wdth=87.5`. Active and partial cases retain two `ss02` features;
the inactive full instance can merge them.
