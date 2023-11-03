use super::{FreeTypeInstance, InstanceOptions, RecordingPen, RegularizingPen, SkrifaInstance};
use skrifa::GlyphId;
use std::{io::Write, path::Path};

pub fn compare_glyphs(
    path: &Path,
    options: &InstanceOptions,
    (mut ft_instance, mut skrifa_instance): (FreeTypeInstance, SkrifaInstance),
    exit_on_fail: bool,
) -> bool {
    let glyph_count = skrifa_instance.glyph_count();
    let is_scaled = options.ppem != 0;

    let mut ft_outline = RecordingPen::default();
    let mut skrifa_outline = RecordingPen::default();

    let mut ok = true;

    for gid in 0..glyph_count {
        let gid = GlyphId::new(gid);
        let ft_advance = ft_instance.advance(gid);
        let skrifa_advance = skrifa_instance.advance(gid);
        if ft_advance != skrifa_advance {
            writeln!(
                std::io::stderr(),
                "[{path:?}#{} ppem: {} coords: {:?}] glyph id {} advance doesn't match:\nFreeType: {ft_advance:?}\nSkrifa:   {skrifa_advance:?}",
                options.index,
                options.ppem,
                options.coords,
                gid.to_u16(),
            )
            .unwrap();
            if exit_on_fail {
                std::process::exit(1);
            }
        }
        ft_outline.clear();
        ft_instance
            .outline(gid, &mut RegularizingPen::new(&mut ft_outline, is_scaled))
            .unwrap();
        skrifa_outline.clear();
        skrifa_instance
            .outline(
                gid,
                &mut RegularizingPen::new(&mut skrifa_outline, is_scaled),
            )
            .unwrap();
        if ft_outline != skrifa_outline {
            ok = false;
            fn outline_to_string(outline: &RecordingPen) -> String {
                outline
                    .0
                    .iter()
                    .map(|cmd| format!("{cmd:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            let ft_cmds = outline_to_string(&ft_outline);
            let skrifa_cmds = outline_to_string(&skrifa_outline);
            let diff = similar::TextDiff::from_lines(&ft_cmds, &skrifa_cmds);
            let mut diff_str = String::default();
            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    similar::ChangeTag::Delete => "-",
                    similar::ChangeTag::Insert => "+",
                    similar::ChangeTag::Equal => " ",
                };
                diff_str.push_str(&format!("{sign} {change}"));
            }
            write!(
                std::io::stderr(),
                "[{path:?}#{} ppem: {} coords: {:?}] glyph id {} outline doesn't match:\n{diff_str}",
                options.index,
                options.ppem,
                options.coords,
                gid.to_u16(),
            )
            .unwrap();
            if exit_on_fail {
                std::process::exit(1);
            }
        }
    }
    ok
}
