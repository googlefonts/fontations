import os
import glob
import re

def fix_imports_skera(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Replace `use write_fonts::{...FontBuilder...};`
    # We can just remove `FontBuilder,` from the write_fonts import
    # and add `use font_builder::FontBuilder;` at the top of the file
    if 'FontBuilder' in content and 'use write_fonts' in content:
        # Remove FontBuilder from write_fonts import block
        content = re.sub(r'FontBuilder,\s*', '', content)
        # Add use font_builder::FontBuilder;
        content = content.replace('use write_fonts::{', 'use font_builder::FontBuilder;\nuse write_fonts::{')
        with open(filepath, 'w') as f:
            f.write(content)

for filepath in glob.glob('skera/src/**/*.rs', recursive=True):
    fix_imports_skera(filepath)

def fix_imports_simple(filepath):
    with open(filepath, 'r') as f:
        content = f.read()
    if 'use write_fonts::FontBuilder;' in content:
        content = content.replace('use write_fonts::FontBuilder;', 'use font_builder::FontBuilder;')
        with open(filepath, 'w') as f:
            f.write(content)

for filepath in glob.glob('incremental-font-transfer/src/**/*.rs', recursive=True):
    fix_imports_simple(filepath)

for filepath in glob.glob('fuzz/fuzz_targets/**/*.rs', recursive=True):
    fix_imports_simple(filepath)

print("Imports updated")
