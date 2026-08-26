# Chromium breakage report

What adopting the parsing rework would cost Chromium. Companion to
[parsing-rework.md](parsing-rework.md) and
[parsing-rework-skia-impact.md](parsing-rework-skia-impact.md).

**Assessed against:** a Chromium checkout at `484e44f3d5` (2026-08-26), which
vendors `read_fonts v0_43`, `skrifa v0_46`, `font_types v0_12` — the same
versions this workspace is on.

**Assumption:** skrifa's public API is held constant, so this covers direct use
of read-fonts only.

## Summary

**Five consumers, of which two have source in the tree, and between them about
six lines change.**

Chromium's exposure is wider than Skia's in *shape* — five GN targets link
read-fonts, and one of them reaches through `skrifa::raw`, the channel Skia
never uses — but narrower than that sounds in practice. Almost everything
Chromium touches is either `TableProvider` (unaffected by design) or a
hand-written read-fonts method that already returns `Option`.

## Who links read-fonts

`//third_party/rust/read_fonts/v0_43:lib` appears in five GN targets:

| target | source | owner |
| --- | --- | --- |
| `skia` | `//third_party/skia/src/ports/fontations/` | Skia — see the [Skia report](parsing-rework-skia-impact.md) |
| `third_party/blink/renderer/platform/fonts` | `opentype/format_check.rs` | Chromium |
| `content/browser` | `font_unique_name_lookup/name_table_ffi.rs` | Chromium |
| `third_party/harfbuzz` | `src/src/rust/{font,hb,lib,shape}.rs` | HarfBuzz upstream |
| `third_party/fontconfig` | `src/fc-fontations/*.rs` | fontconfig upstream |

Two of these are separate upstream projects that would each need their own
patch, on their own schedule. That is a coordination cost more than a code one.

**Note:** the fontconfig sources are named in `BUILD.gn` but are not present in
this checkout, so `fc-fontations` is **unassessed**. It is the one real gap in
this report.

## Chromium's own code

### `blink/renderer/platform/fonts/opentype/format_check.rs` — 90 lines

```rust
use read_fonts::{FileRef, FontRef, ReadError, TableProvider};
```

Detects font formats for Blink. Everything it does is table-presence and a
version word:

```rust
font.table_directory().table_records().iter().map(|e| e.tag())
font_ref.colr().ok()?.version()
font_ref.avar().ok()?.version()
```

**Unaffected.** `FileRef`/`FontRef`/`ReadError` are unchanged, `TableProvider`
keeps returning `Result` by design, `table_records()` is a non-conditional array
already returning `&'a [TableRecord]`, and `version()` is a scalar inside
`MIN_SIZE` on both tables.

### `content/browser/font_unique_name_lookup/name_table_ffi.rs` — 70 lines

```rust
use read_fonts::{FileRef, FontRef, ReadError};
use skrifa::{string::StringId, MetadataProvider};
```

Indexes font names. Its only structural read is:

```rust
font_ref.table_directory.table_records().iter().map(|item| item.offset).min()
```

**Unaffected**, for the same reasons — plus note it reads `table_directory` as a
*field*, not through an accessor, and `TableRecord::offset` is a zerocopy record
field.

## HarfBuzz — `src/src/rust/font.rs`

The one place in the whole tree that uses `skrifa::raw`:

```rust
use skrifa::raw::tables::vmtx::Vmtx;
use skrifa::raw::tables::vorg::Vorg;
use skrifa::raw::tables::vvar::Vvar;
use skrifa::raw::TableProvider;
```

It holds the three vertical-metrics tables and calls four methods:

| call | signature today | after |
| --- | --- | --- |
| `font_ref.vmtx()`, `.vorg()`, `.vvar()` | `Result<T, ReadError>` | unchanged — `TableProvider` |
| `vert_metrics.advance(gid)` | `Option<u16>` | unchanged — hand-written, already `Option` |
| `vert_origin.vertical_origin_y(gid)` | `i16` | unchanged — hand-written, bare |
| `vert_vars.advance_height_delta(gid, coords)` | `Result<Fixed, ReadError>` | `Option<Fixed>` **if** brought into line |
| `vert_vars.v_org_delta(gid, coords)` | `Result<Fixed, ReadError>` | `Option<Fixed>` **if** brought into line |

The last two are **hand-written** in `read-fonts/src/tables/vvar.rs`, not
generated, so they change only if that is done deliberately. Both call sites end
in `.unwrap_or_default()`:

```rust
advance += vert_vars.advance_height_delta(glyph_id, coords).unwrap_or_default().to_f32();
y_origin += vert_vars.v_org_delta(glyph_id, coords).unwrap_or_default().to_f32();
```

`Option::unwrap_or_default` and `Result::unwrap_or_default` are both in scope
and both compile — so **even if those two are converted, these two lines do not
change.** HarfBuzz's breakage is zero either way.

That is worth dwelling on: `unwrap_or_default()` is the one idiom that is
source-compatible across the `Result` → `Option` change. Anywhere a consumer
already writes it, the rework is invisible.

## What actually breaks

Nothing in Chromium's own code. Nothing in HarfBuzz. The breakage in this tree
is **Skia's four lines**, reached through the vendored
`third_party/skia/src/ports/fontations/` — see the
[Skia report](parsing-rework-skia-impact.md) for those, plus two more if
`vvar`'s hand-written deltas are converted and some *other* consumer is not
using `unwrap_or_default`.

## Why Chromium comes off lighter than its dependency count suggests

Three reasons, in order of how much they carry:

1. **`TableProvider` stays on `Result`.** Four of the five consumers use
   read-fonts almost exclusively to ask "does this font have table X", which is
   exactly the boundary the design preserves.
2. **The hand-written accessors already return `Option`.** `Vmtx::advance`,
   `Vorg::vertical_origin_y` and friends were written by hand and already have
   the shape the rework generalises. The rework is, in part, making the
   generated code look like the hand-written code.
3. **`unwrap_or_default()` is shape-agnostic.** It works on both `Result` and
   `Option`, so any call site already using it survives untouched.

## Gaps in this report

- **fontconfig's `fc-fontations`** is listed in `BUILD.gn` but its sources are
  not in this checkout. Unassessed. It is a names/pattern backend, so the
  expectation is `TableProvider` plus `name`-table reads — the pattern that came
  out clean everywhere else — but that is an expectation, not a measurement.
- **The C++ side** was not examined. None of the affected calls appear in a
  signature crossing an FFI boundary, so no C++ should need touching, but that
  was reasoned about rather than compiled.

## Vendored but not built

`third_party/rust/` also carries `write_fonts`, `skera` and
`incremental_font_transfer/v0_7`. Following the GN edges, the chain terminates:

```
write_fonts  <-  skera  <-  incremental_font_transfer  <-  (nothing)
```

No target outside those three crates' own `BUILD.gn` files depends on any of
them, so none is built into Chrome. This matters because
`incremental_font_transfer` is by some distance the heaviest read-fonts consumer
in the workspace — 78 `ReadError` mentions — and would dominate this report if
it were live. It is not, today. If Chromium ever turns IFT on, re-run this.

## Reproducing

```
cd <chromium>/src
git grep -l "read_fonts" -- '*.rs' ':(exclude)third_party/rust'
git grep -l "read_fonts" -- '*.gn' '*.gni' ':(exclude)third_party/rust'
git grep -n "skrifa::raw" -- '*.rs' ':(exclude)third_party/rust'
```

The third command is the important one: `skrifa::raw` is how read-fonts reaches
a consumer that does not name it directly.
