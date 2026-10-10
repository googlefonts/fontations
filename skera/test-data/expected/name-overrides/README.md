# Name overrides reference

`name.bin` is the raw naming table produced by HarfBuzz d659591a6,
with experimental APIs enabled, from `Roboto-Regular.ttf`.
Select Unicode U+0061–U+0063, source name IDs 0–6, English (0x0409),
and the default layout features and subset flags. Apply these overrides
in order through `hb_subset_input_override_name_table`:

| Name ID | Platform | Encoding | Language | Text |
| --- | --- | --- | --- | --- |
| 1 | 3 | 1 | 0x0409 | `Renamed Café 😀` |
| 6 | 0 | 3 | 0 | `Renamed-Regular` |
| 300 | 3 | 1 | 0x0410 | `Nome italiano` |
| 1 | 1 | 0 | 0 | `ASCII family` |
| 4 | 3 | 1 | 0x0409 | NULL (remove) |
| 5 | 3 | 1 | 0x0409 | Empty string (remove) |
| 300 | 3 | 1 | 0x0410 | `Nome finale` |
| 301 | 0 | 4 | 0 | `Embedded\0NUL` (12 UTF-8 bytes, with a literal NUL) |

The test compares record keys and encoded strings, independently of string
storage offsets. This covers surrogate pairs, embedded NULs, repeated
requests, removals, and insertion despite name, language, and platform
selection filters.
