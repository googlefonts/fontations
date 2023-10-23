use std::{
    borrow::Borrow,
    ffi::{c_int, c_void},
    path::{Path, PathBuf},
    sync::Arc,
};

use freetype::{
    face::LoadFlag,
    ffi::{FT_Long, FT_Vector},
    Face, Library,
};

use skrifa::{
    prelude::Size,
    raw::{
        types::{F2Dot14, Pen},
        FileRef, FontRef, TableProvider,
    },
    scale, GlyphId,
};

pub struct FreeTypeFont<'a> {
    face: &'a mut Face<SharedFontData>,
    load_flags: LoadFlag,
}

impl<'a> FreeTypeFont<'a> {
    pub fn glyph_count(&self) -> u16 {
        self.face.num_glyphs() as u16
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.face
            .load_glyph(glyph_id.to_u16() as u32, self.load_flags)
            .ok()?;
        let mut ft_pen = FreeTypePen(pen, !self.load_flags.contains(LoadFlag::NO_SCALE));
        let funcs = freetype::freetype_sys::FT_Outline_Funcs {
            move_to: ft_move_to,
            line_to: ft_line_to,
            conic_to: ft_conic_to,
            cubic_to: ft_cubic_to,
            delta: 0,
            shift: 0,
        };
        unsafe {
            freetype::freetype_sys::FT_Outline_Decompose(
                &self.face.glyph().raw().outline as *const _ as *mut _,
                &funcs,
                (&mut ft_pen) as *mut FreeTypePen as *mut _,
            );
        }
        Some(())
    }
}

pub struct SkrifaFont<'a> {
    font: FontRef<'a>,
    scaler: scale::Scaler<'a>,
}

impl<'a> SkrifaFont<'a> {
    pub fn glyph_count(&self) -> u16 {
        self.font
            .maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default()
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.scaler.outline(glyph_id, pen).ok()?;
        Some(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct FontInstance<'a> {
    pub index: usize,
    pub ppem: u32,
    pub coords: &'a [F2Dot14],
}

impl<'a> FontInstance<'a> {
    pub fn new(index: usize, ppem: u32, coords: &'a [F2Dot14]) -> Self {
        Self {
            index,
            ppem,
            coords,
        }
    }
}

pub struct FontFileData {
    path: PathBuf,
    data: SharedFontData,
    _ft_library: Library,
    ft_faces: Vec<Face<SharedFontData>>,
    skrifa_cx: scale::Context,
}

impl FontFileData {
    pub fn new(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref().to_owned();
        let file = std::fs::File::open(&path).ok()?;
        let data = SharedFontData(unsafe { Arc::new(memmap2::Mmap::map(&file).ok()?) });
        let ft_library = Library::init().unwrap();
        let mut ft_fonts = vec![];
        let count = match FileRef::new(data.0.as_ref()).ok()? {
            FileRef::Font(_) => 1,
            FileRef::Collection(collection) => collection.len(),
        };
        for i in 0..count {
            ft_fonts.push(ft_library.new_memory_face2(data.clone(), i as isize).ok()?);
        }
        Some(Self {
            path,
            data,
            _ft_library: ft_library,
            ft_faces: ft_fonts,
            skrifa_cx: scale::Context::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn count(&self) -> usize {
        self.ft_faces.len()
    }

    pub fn axis_count(&self, index: usize) -> u16 {
        FontRef::from_index(self.data.0.as_ref(), index as u32)
            .and_then(|font| font.fvar())
            .map(|fvar| fvar.axis_count())
            .unwrap_or_default()
    }

    pub fn get(&mut self, instance: &FontInstance) -> Option<(FreeTypeFont, SkrifaFont)> {
        let ft_font = self.ft_faces.get_mut(instance.index)?;
        let mut load_flags = LoadFlag::NO_AUTOHINT | LoadFlag::NO_HINTING | LoadFlag::NO_BITMAP;
        // let upem = ft_font.raw().units_per_EM as u32;
        if instance.ppem != 0 {
            ft_font.set_pixel_sizes(instance.ppem, instance.ppem).ok()?;
        } else {
            load_flags |= LoadFlag::NO_SCALE;
        }
        if !instance.coords.is_empty() {
            let mut ft_coords = vec![];
            ft_coords.extend(
                instance
                    .coords
                    .iter()
                    .map(|c| c.to_fixed().to_bits() as FT_Long),
            );
            unsafe {
                freetype::freetype_sys::FT_Set_Var_Blend_Coordinates(
                    ft_font.raw_mut() as _,
                    instance.coords.len() as u32,
                    ft_coords.as_ptr(),
                );
            }
        } else {
            unsafe {
                freetype::freetype_sys::FT_Set_Var_Design_Coordinates(
                    ft_font.raw_mut() as _,
                    0,
                    std::ptr::null(),
                );
            }
        }
        let font_ref = FontRef::from_index(self.data.0.as_ref(), instance.index as u32).ok()?;
        let size = if instance.ppem != 0 {
            Size::new(instance.ppem as f32)
        } else {
            // Size::new(upem as f32)
            Size::unscaled()
        };
        let scaler = self
            .skrifa_cx
            .new_scaler()
            .size(size)
            .normalized_coords(instance.coords)
            .build(&font_ref);
        Some((
            FreeTypeFont {
                face: ft_font,
                load_flags,
            },
            SkrifaFont {
                font: font_ref,
                scaler,
            },
        ))
    }
}

#[derive(Clone)]
pub struct SharedFontData(Arc<memmap2::Mmap>);

impl Borrow<[u8]> for SharedFontData {
    fn borrow(&self) -> &[u8] {
        self.0.as_ref()
    }
}

struct FreeTypePen<'a>(&'a mut dyn Pen, bool);

impl<'a> FreeTypePen<'a> {
    fn scale_point(&self, p: *const FT_Vector) -> (f32, f32) {
        let p = unsafe { &*p };
        if self.1 {
            const SCALE: f32 = 1.0 / 64.0;
            (p.x as f32 * SCALE, p.y as f32 * SCALE)
        } else {
            (p.x as f32, p.y as f32)
        }
    }
}

fn ft_pen<'a>(user: *mut c_void) -> &'a mut FreeTypePen<'a> {
    unsafe { &mut *(user as *mut FreeTypePen) }
}

extern "C" fn ft_move_to(to: *const FT_Vector, user: *mut c_void) -> c_int {
    let pen = ft_pen(user);
    let (x, y) = pen.scale_point(to);
    pen.0.move_to(x, y);
    0
}

extern "C" fn ft_line_to(to: *const FT_Vector, user: *mut c_void) -> c_int {
    let pen = ft_pen(user);
    let (x, y) = pen.scale_point(to);
    pen.0.line_to(x, y);
    0
}

extern "C" fn ft_conic_to(
    control: *const FT_Vector,
    to: *const FT_Vector,
    user: *mut c_void,
) -> c_int {
    let pen = ft_pen(user);
    let (cx0, cy0) = pen.scale_point(control);
    let (x, y) = pen.scale_point(to);
    pen.0.quad_to(cx0, cy0, x, y);
    0
}

extern "C" fn ft_cubic_to(
    control1: *const FT_Vector,
    control2: *const FT_Vector,
    to: *const FT_Vector,
    user: *mut c_void,
) -> c_int {
    let pen = ft_pen(user);
    let (cx0, cy0) = pen.scale_point(control1);
    let (cx1, cy1) = pen.scale_point(control2);
    let (x, y) = pen.scale_point(to);
    pen.0.curve_to(cx0, cy0, cx1, cy1, x, y);
    0
}
