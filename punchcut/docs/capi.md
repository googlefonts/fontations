# C API for punchcut

This is a rough proposal for a C API for punchcut.

## Header

```c
#include <stdint.h>
#include <stddef.h>

struct PcContext;
struct PcScalerBuilder;
struct PcScaler;
struct PcOutline;

// Supported hinting modes.
enum PcHinting {
    // No hinting.
    PC_HINTING_NONE = 0,
    // Full gridfitting.
    PC_HINTING_LEGACY = 1,
    // Grayscale antialiasing (FreeType light + grayscale)
    PC_HINTING_GRAYSCALE_SUBPIXEL = 2,
    // RGB subpixel hinting (FreeType light + LCD)
    PC_HINTING_SUBPIXEL = 3,
    // Same as RGB subpixel, but prevents all movement
    // in the horizontal direction.
    PC_HINTING_MODERN = 4,
};

// Reference to a font.
struct PcFont {
    // Pointer to full content of font tile.
    const char* data;
    // Size in bytes of data.
    size_t data_size;
    // Index in a font collection.
    uint32_t index;
};

// Result codes.
enum PcResult {
    PC_OK = 0,
    PC_ERROR_NO_SOURCES,
    PC_ERROR_GLYPH_NOT_FOUND,
    PC_ERROR_RECURSION_LIMIT_EXCEEDED,
    PC_ERROR_HINTING_FAILED,
    PC_ERROR_INVALID_ANCHOR_POINT,
    PC_ERROR_READ,
};

// Simple 2D point structure.
struct PcPoint {
    float x;
    float y;
};

// Actions of a path element.
enum PcVerb {
    PC_VERB_MOVE_TO,
    PC_VERB_LINE_TO,
    PC_VERB_QUAD_TO,
    PC_VERB_CURVE_TO,
    PC_VERB_CLOSE,
};

// Callback for path element enumeration.
typedef void (*PcPathElementFunc)(PcVerb, PcPoint*, void*);

// Creates a new context for scaling glyphs. This maintains both a hinting
// cache and internal storage to reduce allocation cost.
PcContext* pc_context_new();

// Destroys the context.
void pc_context_destroy(PcContext* context);

// Creates a new builder for configuring a scaler with the given context. 
PcScalerBuilder* pc_scaler_builder_new(PcContext* context);

// Sets a unique font identifier to enable internal caching of hinting state.
void pc_scaler_builder_set_font_id(PcScalerBuilder* builder, uint64_t font_id);

// Sets the font size in pixels per em. A size of 0.0 will disable scaling and
// the resulting scaler will generate outlines in font units.
void pc_scaler_builder_set_size(PcScalerBuilder* builder, float ppem);

// Sets the desired hinting mode.
void pc_scaler_builder_set_hinting(PcScalerBuilder* builder, PcHinting hinting);

// Adds a variation setting for the given tag and value.
void pc_scaler_builder_add_variation(PcScalerBuilder* builder, uint32_t tag, float value);

// Builds a scaler for the current configuration and the given font. This
// consumes the builder.
PcScaler* pc_scaler_builder_build(PcScalerBuilder* builder, const PcFont* font);

// Loads an outline for the specified glyph and stores the result in the given target.
PcResult pc_scaler_outline(PcScaler* scaler, uint32_t glyph_id, PcOutline* outline);

// Destroys the scaler.
void pc_scaler_destroy(PcScaler* scaler);

// Creates a new empty outline.
PcOutline* pc_outline_new();

// Calls the given function for each path element in the outline. The value of the user
// parameter is passed as the third argument.
void pc_outline_path_elements(const PcOutline* outline, void* user, PcPathElementFunc func);

// Destroys the outline.
void pc_outline_destroy(PcOutline* outline);

```

## Example usage

```c
// Create a new context. These can be temporary per run, or kept in some
// thread local storage.
PcContext* context = pc_context_new();

// Create an outline for storing the result.
PcOutline* outline = pc_outline_new();

// Create a font reference.
PcFont font = { .. };

// Create a new builder.
PcScalerBuilder* builder = pc_scaler_builder_new(context);
// Set the font size to 16 ppem.
pc_scaler_builder_set_size(builder, 16.0f);
// Set the hinting mode to "subpixel". Equivalent to FreeType
// light hinting.
pc_scaler_builder_set_hinting(builder, PC_HINTING_SUBPIXEL);

// Create the scaler. This consumes the builder.
PcScaler* scaler = pc_scaler_builder_build(builder, &font);

// Loop over some glyphs in a run... assuming an array called
// glyph containing ids with n_glyphs holding the array length.
for (int i = 0; i < n_glyphs; i++) {
    if pc_scaler_outline(scaler, glyphs[i], outline) == PC_OK {
        // Store in a Skia path.
        SkPath path;
        pc_outline_path_elements(outline, &path, sk_path_outline_func);
    }
}

// Cleanup!
pc_scaler_destroy(scaler);
pc_outline_destroy(outline);
pc_context_destroy(context);

```