#include <stdio.h>
#include <fstream>
#include <iterator>
#include <vector>
#include "outlines.h"
#include "skrifa_cxx/src/main.rs.h"

void dump_outline(skrifa::Outline& outline);

void skrifa::run() {
    // Load the font data
    std::ifstream input("../font-test-data/test_data/type1/notoserif-regular.subset.pfa", std::ios::binary);
    std::vector<char> bytes(
         (std::istreambuf_iterator<char>(input)),
         (std::istreambuf_iterator<char>()));
    input.close();

    // Create a rust slice containing the font data
    rust::Slice<const uint8_t> slice((const uint8_t*)bytes.data(), bytes.size());

    // Load the font. Note that the bytes vector must live as long as the font
    auto font = skrifa::new_ps_font(slice);
    if (!font->is_ok()) {
        printf("Failed to load font!\n");
        return;
    }

    // Get the PostScript name
    auto name = (std::string)font->name();

    // And the family name
    auto family_name = (std::string)font->family_name();

    printf("ps name = %s, family name = %s\n", name.c_str(), family_name.c_str());

    // How many glyphs?
    printf("Font has %d glyphs\n", font->num_glyphs());

    // The font's encoding is Adobe standard
    assert(font->encoding() == PsEncodingKind::Standard);

    // The Adobe standard code for 'x' is 120
    auto gid_from_code = font->code_to_gid(120);

    // But we also generate a Unicode mapping for glyphs present in the AGL
    auto gid = font->unicode_to_gid('x');

    assert(gid_from_code == gid);

    // Storage for our outline
    skrifa::Outline outline;

    // Load an unscaled outline
    font->unscaled_outline(gid, outline);
    dump_outline(outline);

    // Load the same outline at 16px  
    font->scaled_outline(gid, 16.0, outline);
    dump_outline(outline);

    // Convert glyph name to unicode
    uint32_t period_unicode;
    assert(skrifa::agl_name_to_unicode("period", period_unicode));
    assert(period_unicode == '.');

    // Convert unicode to glyph name
    uint8_t period_name[64];
    assert(skrifa::agl_unicode_to_name('.', rust::Slice<uint8_t>(period_name, 40)));
    assert(!strcmp((char*)&period_name[0], "period"));
}

void dump_outline(skrifa::Outline& outline) {
    auto point_idx = 0;
    for (auto verb : outline.verbs) {
        switch (verb) {
            case skrifa::PathVerb::MoveTo: {
                auto p = outline.points[point_idx];
                point_idx += 1;
                printf("M%f,%f ", p.x, p.y);
                break;
            }
            case skrifa::PathVerb::LineTo: {
                auto p = outline.points[point_idx];
                point_idx += 1;
                printf("L%f,%f ", p.x, p.y);
                break;      
            }
            case skrifa::PathVerb::QuadTo: {
                auto c0 = outline.points[point_idx];
                auto p = outline.points[point_idx + 1];
                point_idx += 2;
                printf("Q%f,%f %f,%f ", c0.x, c0.y, p.x, p.y);
                break;
            }
            case skrifa::PathVerb::CurveTo: {
                auto c0 = outline.points[point_idx];
                auto c1 = outline.points[point_idx + 1];
                auto p = outline.points[point_idx + 2];
                point_idx += 3;
                printf("C%f,%f %f,%f %f,%f ", c0.x, c0.y, c1.x, c1.y, p.x, p.y);
                break;       
            }
            case skrifa::PathVerb::Close:
                printf("Z");
                break;                                    
            default: 
                break;
        }
    }
    printf("\nadvance = %f\n", outline.advance_width);
}
