# skera

`skera` is a Rust library and binary for subsetting a font file according to provided input.

Use `--keep-everything` or the library's `Plan::keep_everything` to select all
glyphs, Unicode mappings, names, layout items, and tables. The preset also
preserves glyph names, legacy naming records, the `.notdef` outline, and
Unicode range bits. CLI selectors explicitly supplied alongside the preset
replace their corresponding selections; other flags add to the preset.
Tables are still subset and can be optimized or re-encoded.

Custom glyph IDs are supported through `--gid-map 1:4,2:7` or
`Plan::set_glyph_mapping`. Unspecified retained glyphs follow the highest
requested output ID; gaps become empty glyphs. The final mapping must preserve
original glyph order, keep `.notdef` at zero, and use unique output IDs.
Custom mappings cannot be combined with `--retain-gids`. Add
`--retain-num-glyphs` alongside `--retain-gids` to keep the source glyph count
by appending empty glyphs. `--iftb-requirements` forces 32-bit `loca`, `gvar`,
and CFF/CFF2 CharStrings offsets for incremental font transfer patches.

`--no-bidi-closure` omits mirrored Unicode variants from the subset.
Use `--name-IDs` and `--name-languages` to select naming records;
`--name-legacy` also keeps records for non-Unicode platforms.
The library's `Plan::override_name_table` replaces or inserts individual
records, including records excluded by those filters. Passing `None` or an
empty string removes a record. Macintosh overrides accept ASCII text;
other platforms encode text as UTF-16BE.

CFF1 and CFF2 outlines support subsetting with retained subroutines, optional
desubroutinization (`--desubroutinize`), hint removal (`--no-hinting`), and
retained glyph IDs (`--retain-gids`). CFF1 subsetting includes CID fonts,
custom and expert encodings, glyph names, and `seac` component closure.

VARC subsetting closes component references and remaps glyphs, conditions,
axis-index lists, variation rows, and sparse regions. Packed component values,
transforms, and reserved fields are preserved. Axes that retained VARC data
does not reference can be pinned or restricted. Component-axis lists, sparse
regions, and nested condition axes are remapped when axes are removed.
Changes to referenced axes are rejected. For library calls, subset with
retained glyph IDs before instancing to remove unused VARC references.

MATH subsetting retains the variants and assembly parts of selected glyphs,
remaps their references, and preserves constants, per-glyph math values,
kerning and device adjustments.

CFF2 and TrueType fonts can be fully instantiated with `--instance wght=650,opsz=48`.
TrueType instancing preserves composite glyphs, point order, and hint programs,
and applies `gvar` phantom-point metrics and `cvar` control-value deltas.
The optional `spec_next` feature preserves experimental cubic glyf control
points through full and partial instancing.
Unspecified axes are retained; `tag=drop` pins an axis at its default. Partial
instancing accepts ranges such as `--instance wght=300:500:700,opsz=12:48`.
Instancing resolves outline blends and variation deltas in metrics, layout,
and COLRv1 paint tables.
Nested CFF2 blends, including variable deltas, are preserved through subsetting
and partial instancing; full instancing rounds each blend independently.
The library's `instance_font` preserves glyph IDs; create a `Plan` from the
returned font to subset the instance.

Add `--downgrade-cff2` to emit a full instance as CID-keyed CFF1. The library
also exposes `downgrade_cff2`; it preserves glyph IDs and encodes CFF1 widths
from the instanced metrics.

Full and partial instancing support `avar` versions 1 and 2. For coupled
`avar` version 2 maps, TrueType pins whose final coordinates are constant
are removed and baked into outlines, metrics, and layout. Other pins remain
hidden until the font is fully instantiated. Range compensation preserves
the original final-coordinate space, subject to F2Dot14 quantization.

For font collections, `--face-index` selects a face; the default is zero.

## Installation

### Library

To use `skera` in your Rust project, add it via `cargo`:

```bash
cargo add skera
```

### CLI

To install the `skera` command-line tool, use `cargo install` with the `cli` feature enabled:

```bash
cargo install skera --features cli
```

## Usage

### CLI

To subset a font using the command-line tool:

```bash
skera --path <INPUT_PATH> --unicodes <UNICODES> --output-file <OUTPUT_PATH>
```

For a full list of available options and flags, run:

```bash
skera --help
```

## Profiling

How-To: Profile a Skera Binary using Samply

### Prerequisites
Install samply using Cargo:

```bash
cargo install samply
```

### Step 1: Enable Debug Symbols in Release Profiles
Add the following configuration to your root Cargo.toml:

```bash
[profile.release]
debug = true
```

### Step 2: Compile your crate using the release profile with the cli feature:

```bash
cargo build --release -p skera --features="cli"
```

### Step 3: Record and Profile the Binary
Run the generated release executable using samply record. Replace <bin> with your compiled binary's name and <args...> with any execution arguments:

```bash
samply record target/release/skera <args...>
```

example:

```bash
samply record target/release/skera font-file --unicodes=* --output-file=out
```

### Step 4: Analyze the Results
Once your binary finishes execution, samply automatically starts a local web server and prints a URL to the console:
```bash
Server listening on http://127.0.0.1:3000
Press Ctrl+C to stop.
```

Open the provided link in your web browser.
The browser will open the Firefox Profiler interface, populated with your recorded run's call tree, flame graph, and timeline views.

### Alternative Profiling Tools
If samply is not suitable for your target environment, you may consider these alternative options:

cargo-flamegraph: A tool that utilizes perf (on Linux) or dtrace (on macOS) to generate a static vector-graphic .svg flame graph.

```bash
cargo install cargo-flamegraph
cargo flamegraph -p skera --features="cli"
```

perf (Linux-only): The standard system profiler on Linux, useful for command-line profiling and capturing kernel-level events.

```bash
perf record -g -- target/release/skera
perf report
```
