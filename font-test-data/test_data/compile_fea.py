import sys
import os

from fontTools.ttLib import TTFont

from fontTools.feaLib.builder import addOpenTypeFeatures

def makeTTFont(glyph_list_path):
    glyphs = get_glyph_list(glyph_list_path)
    font = TTFont()
    font.setGlyphOrder(glyphs)
    return font

def get_glyph_list(path):
    with open(path) as f:
        lines = f.read().splitlines()
    return [l for l in lines if not l.startswith("#")]

def main():
    try:
        fea_path = sys.argv[1]
        out_path = sys.argv[2]
    except IndexError:
        print("Usage: compile_fea.py <fea_file> <out_file>")
        sys.exit(1)


    if not os.path.exists(fea_path):
        print("Feature file not found: " + fea_path)
        sys.exit(1)
    glyph_list_path = os.path.splitext(fea_path)[0] + "_glyphs.txt"
    if not os.path.exists(glyph_list_path):
        print("Glyph list file not found: " + glyph_list_path)
        sys.exit(1)

    font = makeTTFont((glyph_list_path))
    addOpenTypeFeatures(font, fea_path)
    # if you want to manually inspect you can dump as TTX:
    # font.saveXML(out_path, tables=[ 'GDEF', 'GSUB', 'GPOS'])
    font.save(out_path)

if __name__ == "__main__":
    main()
