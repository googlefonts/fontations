use std::{
    borrow::Borrow,
    path::{Path, PathBuf},
    sync::Arc,
};

use ::freetype::Library;
use ::skrifa::{
    outline::HintingMode,
    raw::{types::F2Dot14, FileRef, FontRef, TableProvider},
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
    pub hinting: Option<HintingMode>,
}

impl<'a> InstanceOptions<'a> {
    pub fn new(
        index: usize,
        ppem: u32,
        coords: &'a [F2Dot14],
        hinting: Option<HintingMode>,
    ) -> Self {
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
    count: usize,
    ft_library: Library,
}

impl Font {
    pub fn new(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref().to_owned();
        let file = std::fs::File::open(&path).ok()?;
        let data = SharedFontData(unsafe { Arc::new(memmap2::Mmap::map(&file).ok()?) });
        let count = match FileRef::new(data.0.as_ref()).ok()? {
            FileRef::Font(_) => 1,
            FileRef::Collection(collection) => collection.len() as usize,
        };
        let _ft_library = ::freetype::Library::init().ok()?;
        Some(Self {
            path,
            data,
            count,
            ft_library: _ft_library,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn count(&self) -> usize {
        self.count
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
        let ft_instance = FreeTypeInstance::new(&self.ft_library, &self.data, options)?;
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
