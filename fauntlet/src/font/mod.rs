use std::{
    borrow::Borrow,
    path::{Path, PathBuf},
    sync::Arc,
};

use ::freetype::{Face, Library};
use ::skrifa::{
    raw::{types::F2Dot14, FontRef, TableProvider},
    Hinting,
};

mod freetype;
mod skrifa;

pub use freetype::FreeTypeInstance;
pub use skrifa::SkrifaInstance;

#[derive(Copy, Clone, Debug)]
pub struct InstanceOptions<'a> {
    pub index: usize,
    pub ppem: u32,
    pub coords: &'a [F2Dot14],
    pub hinting: Hinting,
}

impl<'a> InstanceOptions<'a> {
    pub fn new(index: usize, ppem: u32, coords: &'a [F2Dot14], hinting: Hinting) -> Self {
        Self {
            index,
            ppem,
            coords,
            hinting,
        }
    }
}

pub struct Font {
    path: PathBuf,
    data: SharedFontData,
    // Just to keep the FT_Library alive
    _ft_library: Library,
    ft_faces: Vec<Face<SharedFontData>>,
}

impl Font {
    pub fn new(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref().to_owned();
        let file = std::fs::File::open(&path).ok()?;
        let data = SharedFontData(unsafe { Arc::new(memmap2::Mmap::map(&file).ok()?) });
        let (_ft_library, ft_faces) = freetype::collect_faces(&data)?;
        Some(Self {
            path,
            data,
            _ft_library,
            ft_faces,
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
        let ft_instance = FreeTypeInstance::new(face, options)?;
        let skrifa_instance = SkrifaInstance::new(&self.data, options)?;
        Some((ft_instance, skrifa_instance))
    }
}

#[derive(Clone)]
pub struct SharedFontData(Arc<memmap2::Mmap>);

impl Borrow<[u8]> for SharedFontData {
    fn borrow(&self) -> &[u8] {
        self.0.as_ref()
    }
}
