use freetype::{
    face::LoadFlag,
    ffi::{FT_Long, FT_Vector},
    Face, Library,
};
use skrifa::{raw::FileRef, scale::Pen, GlyphId};

use std::ffi::{c_int, c_void};

use super::{InstanceOptions, SharedFontData};

pub fn collect_faces(data: &SharedFontData) -> Option<(Library, Vec<Face<SharedFontData>>)> {
    let library = Library::init().unwrap();
    let mut faces = vec![];
    let font_count = match FileRef::new(data.0.as_ref()).ok()? {
        FileRef::Font(_) => 1,
        FileRef::Collection(collection) => collection.len(),
    };
    for i in 0..font_count {
        faces.push(library.new_memory_face2(data.clone(), i as isize).ok()?);
    }
    Some((library, faces))
}

pub struct FreeTypeInstance<'a> {
    face: &'a mut Face<SharedFontData>,
    load_flags: LoadFlag,
}

impl<'a> FreeTypeInstance<'a> {
    pub fn new(face: &'a mut Face<SharedFontData>, options: &InstanceOptions) -> Option<Self> {
        let mut load_flags = LoadFlag::NO_AUTOHINT | LoadFlag::NO_HINTING | LoadFlag::NO_BITMAP;
        if options.ppem != 0 {
            face.set_pixel_sizes(options.ppem, options.ppem).ok()?;
        } else {
            load_flags |= LoadFlag::NO_SCALE;
        }
        if !options.coords.is_empty() {
            let mut ft_coords = vec![];
            ft_coords.extend(
                options
                    .coords
                    .iter()
                    .map(|c| c.to_fixed().to_bits() as FT_Long),
            );
            unsafe {
                freetype::freetype_sys::FT_Set_Var_Blend_Coordinates(
                    face.raw_mut() as _,
                    options.coords.len() as u32,
                    ft_coords.as_ptr(),
                );
            }
        } else {
            unsafe {
                // Note the explicit call to set *design* coordinates. Setting
                // blend doesn't correctly disable variation processing
                freetype::freetype_sys::FT_Set_Var_Design_Coordinates(
                    face.raw_mut() as _,
                    0,
                    std::ptr::null(),
                );
            }
        }
        Some(Self { face, load_flags })
    }

    pub fn is_scalable(&self) -> bool {
        self.face.is_scalable()
    }

    pub fn glyph_count(&self) -> u16 {
        self.face.num_glyphs() as u16
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        let is_scaled = !self.load_flags.contains(LoadFlag::NO_SCALE);
        let mut load_flags = self.load_flags();
        if !is_scaled {
            // Without this load flag, FT applies scale to linearHoriAdvance
            load_flags |= LoadFlag::LINEAR_DESIGN;
        }
        self.face
            .load_glyph(glyph_id.to_u16() as u32, load_flags)
            .ok()?;
        let advance = self.face.glyph().linear_hori_advance() as f32;
        Some(if is_scaled {
            advance / 65536.0
        } else {
            advance
        })
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.face
            .load_glyph(glyph_id.to_u16() as u32, self.load_flags())
            .ok()?;
        let mut ft_pen = FreeTypePen {
            inner: pen,
            is_scaled: !self.load_flags.contains(LoadFlag::NO_SCALE),
        };
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

    fn load_flags(&self) -> LoadFlag {
        // LoadFlag isn't Copy or Clone?
        LoadFlag::from_bits_truncate(self.load_flags.bits())
    }
}

// Since Pen is dyn here which is a fat pointer, we wrap it in a struct
// to pass through the required void* type in FT_Outline_Decompose.
struct FreeTypePen<'a> {
    inner: &'a mut dyn Pen,
    is_scaled: bool,
}

impl<'a> FreeTypePen<'a> {
    fn scale_point(&self, p: *const FT_Vector) -> (f32, f32) {
        let p = unsafe { &*p };
        if self.is_scaled {
            const SCALE: f32 = 1.0 / 64.0;
            (p.x as f32 * SCALE, p.y as f32 * SCALE)
        } else {
            (p.x as f32, p.y as f32)
        }
    }
}

fn ft_pen<'a>(user: *mut c_void) -> &'a mut FreeTypePen<'a> {
    // SAFETY: this is wildly unsafe and only works if we make sure to pass
    // &mut FreeTypePen as the user parameter to FT_Outline_Decompose
    unsafe { &mut *(user as *mut FreeTypePen) }
}

extern "C" fn ft_move_to(to: *const FT_Vector, user: *mut c_void) -> c_int {
    let pen = ft_pen(user);
    let (x, y) = pen.scale_point(to);
    pen.inner.move_to(x, y);
    0
}

extern "C" fn ft_line_to(to: *const FT_Vector, user: *mut c_void) -> c_int {
    let pen = ft_pen(user);
    let (x, y) = pen.scale_point(to);
    pen.inner.line_to(x, y);
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
    pen.inner.quad_to(cx0, cy0, x, y);
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
    pen.inner.curve_to(cx0, cy0, cx1, cy1, x, y);
    0
}
