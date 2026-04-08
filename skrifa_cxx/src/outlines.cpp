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
    // How many glyphs?
    printf("Font as %d glyphs\n", font->num_glyphs());
    // Get gid for x
    auto gid = font->unicode_to_gid('x');
    // Storage for our outline
    skrifa::Outline outline;
    // Load an unscaled outline
    font->unscaled_outline(gid, outline);
    dump_outline(outline);
    // Load an outline at 16px  
    font->scaled_outline(1, 16.0, outline);
    dump_outline(outline);    
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
