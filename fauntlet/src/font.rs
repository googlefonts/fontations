use std::{
    borrow::Borrow,
    ffi::{c_int, c_void},
    path::{Path, PathBuf},
    sync::Arc,
};

use freetype::{
    face::LoadFlag,
    ffi::{FT_Error, FT_Face, FT_Fixed, FT_Int32, FT_Long, FT_UInt, FT_Vector},
    Face, Library,
};

use skrifa::{
    prelude::{LocationRef, Size},
    raw::{
        types::{F2Dot14, Pen},
        FileRef, FontRef, TableProvider,
    },
    scale, GlyphId, MetadataProvider,
};

pub struct FreeTypeInstance<'a> {
    face: &'a mut Face<SharedFontData>,
    load_flags: LoadFlag,
}

impl<'a> FreeTypeInstance<'a> {
    pub fn glyph_count(&self) -> u16 {
        self.face.num_glyphs() as u16
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        let mut advance: FT_Fixed = 0;
        if unsafe {
            FT_Get_Advance(
                self.face.raw_mut(),
                glyph_id.to_u16() as _,
                self.load_flags.bits(),
                &mut advance as *mut _,
            )
        } == 0
        {
            let mut advance = advance as f32;
            if !self.load_flags.contains(LoadFlag::NO_SCALE) {
                advance /= 65536.0;
            }
            return Some(advance);
        }
        None
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.face
            .load_glyph(glyph_id.to_u16() as u32, self.load_flags)
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
}

pub struct SkrifaInstance<'a> {
    font: FontRef<'a>,
    ppem: f32,
    scaler: scale::Scaler<'a>,
}

impl<'a> SkrifaInstance<'a> {
    pub fn glyph_count(&self) -> u16 {
        self.font
            .maxp()
            .map(|maxp| maxp.num_glyphs())
            .unwrap_or_default()
    }

    pub fn advance(&mut self, glyph_id: GlyphId) -> Option<f32> {
        self.font
            .glyph_metrics(
                Size::new(self.ppem),
                LocationRef::new(self.scaler.normalized_coords()),
            )
            .advance_width(glyph_id)
    }

    pub fn outline(&mut self, glyph_id: GlyphId, pen: &mut impl Pen) -> Option<()> {
        self.scaler.outline(glyph_id, pen).ok()?;
        Some(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InstanceOptions<'a> {
    pub index: usize,
    pub ppem: u32,
    pub coords: &'a [F2Dot14],
}

impl<'a> InstanceOptions<'a> {
    pub fn new(index: usize, ppem: u32, coords: &'a [F2Dot14]) -> Self {
        Self {
            index,
            ppem,
            coords,
        }
    }
}

pub struct Font {
    path: PathBuf,
    data: SharedFontData,
    // Just to keep the FT_Library alive
    _ft_library: Library,
    ft_faces: Vec<Face<SharedFontData>>,
    skrifa_cx: scale::Context,
}

impl Font {
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

    /// Create instances for both FreeType and Skrifa with the given options.
    ///
    /// Borrowing rules require both to be created at the same time. The
    /// mutable borrow here prevents the underlying FT_Face from being modified
    /// before it is dropped.
    pub fn instantiate(
        &mut self,
        options: &InstanceOptions,
    ) -> Option<(FreeTypeInstance, SkrifaInstance)> {
        let face = self.ft_faces.get_mut(options.index)?;
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
        let font = FontRef::from_index(self.data.0.as_ref(), options.index as u32).ok()?;
        let size = if options.ppem != 0 {
            Size::new(options.ppem as f32)
        } else {
            Size::unscaled()
        };
        let scaler = self
            .skrifa_cx
            .new_scaler()
            .size(size)
            .normalized_coords(options.coords)
            .build(&font);
        Some((
            FreeTypeInstance { face, load_flags },
            SkrifaInstance {
                font,
                ppem: size.ppem().unwrap_or_default(),
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

extern "C" {
    // freetype-sys doesn't expose this function
    pub fn FT_Get_Advance(
        face: FT_Face,
        gindex: FT_UInt,
        load_flags: FT_Int32,
        padvance: *mut FT_Fixed,
    ) -> FT_Error;
}
