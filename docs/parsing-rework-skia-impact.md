# Skia breakage report

What adopting the parsing rework would cost Skia. Companion to
[parsing-rework.md](parsing-rework.md).

**Assessed against:** `google/skia` upstream `main` at `7788196ddd`
(2026-08-25), which pins `read-fonts 0.40.1`, `font-types 0.12.0`,
`skrifa 0.43.2`.

**Assumption:** skrifa's public API is held constant. skrifa absorbs the churn
internally, so this covers only Skia's *direct* use of read-fonts.

## Summary

**Four lines.**

Skia reaches into read-fonts in five files, all under `src/ports/fontations/`,
and nowhere else in the tree. Of everything it touches, four call sites change
shape — and every one of them gets shorter.

The bridge sits on skrifa for anything structural and drops to read-fonts only
for table-presence checks and a handful of bitmap and colour lookups. That is
close to the best case for a rework this size.

## Where Skia touches read-fonts

`git grep read_fonts` across the whole tree, excluding vendored deps, finds it
in build files and five source files:

| file | what it imports |
| --- | --- |
| `src/ports/fontations/src/base.rs` | `FileRef`, `FontRef`, `ReadError`, `TableProvider` |
| `src/ports/fontations/src/bitmap.rs` | `tables::bitmap::{BitmapContent, BitmapData, BitmapDataFormat, BitmapMetrics, BitmapSize}`, `tables::sbix::{GlyphData, Strike}`, `FontRef`, `TableProvider` |
| `src/ports/fontations/src/colr.rs` | `tables::colr::CompositeMode`, `tables::cpal::Cpal`, `TableProvider` |
| `src/ports/fontations/src/names.rs` | `tables::os2::SelectionFlags`, `TableProvider` |
| `src/ports/fontations/src/ffi.rs` | nothing — one mention, in a comment |

Two facts worth stating explicitly, because they carry most of the result:

- **`skrifa::raw` is used zero times** anywhere in Skia. The re-exported
  read-fonts surface is not a channel for breakage here.
- **`FontData` and `FontRead` are named zero times.** The `FontData` → `Bytes`
  rename and the `FontRead` → `Table` rename do not reach Skia at all.

## What does not break

### `TableProvider` — 19 call sites

`head()`, `os2()`, `post()`, `hhea()`, `maxp()`, `glyf()`, `fvar()`, `colr()`,
`cpal()`, `cblc()`, `cbdt()`, `eblc()`, `ebdt()`, `sbix()`.

The design keeps `ReadError` at exactly this boundary, so these continue to
return `Result<T, ReadError>` and every `.ok()?` and `.is_ok()` still compiles.

**This is the load-bearing assumption of the whole report.** If `TableProvider`
were ever moved to `Option`, these 19 sites move from "unaffected" to
"breaking" and the picture changes completely.

### Scalar accessors

`num_glyphs()`, `units_per_em()`, `fs_type()`, `is_fixed_pitch()`,
`italic_angle()`, `number_of_h_metrics()`, `panose_10()`,
`num_palette_entries()`, `ppem()`, `strike_size()`.

All within their table's `MIN_SIZE`, so bare `T` before and bare `T` after.

### Non-conditional array accessors

`cblc.bitmap_sizes()`, `eblc.bitmap_sizes()`, `cbdt.data()`, `ebdt.data()`.

Already `&'a [T]` via `unwrap_or_default`, and the rework preserves that rule
deliberately — a non-conditional field beyond `MIN_SIZE` keeps returning a value
rather than an `Option`.

## What breaks

Four sites. Line numbers are from `7788196ddd` and will drift.

### 1. `bitmap.rs:97` — offset array iteration

```rust
let mut strikes = sbix.strikes().iter().filter_map(|strike| strike.ok());
```

`Sbix::strikes()` is `ArrayOfOffsets<'a, Strike<'a>, Offset32>`, whose iterator
yields `Result<Strike, ReadError>`. Afterwards it is
`Array<'a, OffsetTo<Strike<'a>, Offset32>>`, yielding `Option<Strike>`.

```rust
let mut strikes = sbix.strikes().iter().flatten();
```

### 2. `bitmap.rs:106` — nested Result/Option

```rust
glyph_data: best_strike.glyph_data(glyph_id).ok()??,
```

`sbix::Strike::glyph_data` returns `Result<Option<GlyphData<'a>>, ReadError>`.
Afterwards `Option<GlyphData<'a>>`.

```rust
glyph_data: best_strike.glyph_data(glyph_id)?,
```

### 3. `bitmap.rs:123` and `bitmap.rs:146` — resolution that can fail

```rust
let location = best_strike.location(cblc.offset_data(), glyph_id).ok()?;
let location = best_strike.location(eblc.offset_data(), glyph_id).ok()?;
```

`bitmap::Strike::location` returns `Result<BitmapLocation, ReadError>`.
Afterwards `Option<BitmapLocation>`. `offset_data()` changes its *type* from
`FontData` to `Bytes`, but Skia never names it, so the call is unchanged.

```rust
let location = best_strike.location(cblc.offset_data(), glyph_id)?;
```

### 4. `colr.rs:329` — the flattening, in miniature

```rust
let color_records = cpal.color_records_array()?.ok()?;
```

A versioned, nullable offset to an array: today
`Option<Result<&'a [ColorRecord], ReadError>>`. Afterwards `Option<&[ColorRecord]>`.

```rust
let color_records = cpal.color_records_array()?;
```

## Notes for whoever does the work

Two of the four — `sbix::Strike::glyph_data` and `bitmap::Strike::location` —
are **hand-written** in read-fonts, not generated. They would change only if
brought into line with the generated accessors deliberately. Leaving them on
`Result` would cut Skia's breakage to two lines, at the cost of an inconsistency
that would be worth more than it saves.

`bitmap.rs` accounts for three of the four. It is the only file in the bridge
that walks a table structure rather than reading a field off one.

## What this report does not cover

- **Chrome.** Not assessed. Its Rust font code lives in the Chromium tree and no
  checkout was available. Chrome reaches fonts mainly through Skia, but if it
  links read-fonts directly anywhere, that is unmeasured. `git grep read_fonts`
  in a Chromium checkout would settle it.
- **Skia's C++ side.** Only the Rust bridge was examined. The `cxx` FFI boundary
  in `ffi.rs` is unaffected — none of the four sites appear in a signature that
  crosses it — so no C++ should need touching, but that was reasoned about
  rather than compiled.
- **Whether the rework ships at all.** This measures the cost if it does.

## Reproducing

```
cd <skia>
git fetch upstream main
git grep read_fonts FETCH_HEAD -- . ':(exclude)third_party/externals'
git grep -c "skrifa::raw" FETCH_HEAD -- '*.rs'
```

Then, for each accessor the bridge calls, compare against the shape the rework
gives it — the three-way rule in
[parsing-rework.md](parsing-rework.md#2-accessors-return-option-and-it-does-not-nest)
is enough to classify any of them without reading the generated code.
