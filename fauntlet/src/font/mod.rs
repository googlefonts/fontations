use std::{
    borrow::Borrow,
    path::{Path, PathBuf},
    sync::Arc,
};

use ::freetype::{face::LoadFlag, Library};
use ::skrifa::{
    outline::{HintingOptions, SmoothMode, Target},
    raw::{types::F2Dot14, FontRef, TableProvider},
};

mod freetype;
mod skrifa;

pub use freetype::FreeTypeInstance;
pub use skrifa::SkrifaInstance;

#[derive(Copy, Clone, PartialEq, Eq, Default, Debug)]
pub enum HintingTarget {
    #[default]
    Normal,
    Light,
    Lcd,
    VerticalLcd,
    Mono,
}

impl HintingTarget {
    pub fn to_skrifa_target(self) -> Target {
        match self {
            Self::Normal => SmoothMode::Normal.into(),
            Self::Light => SmoothMode::Light.into(),
            Self::Lcd => SmoothMode::Lcd.into(),
            Self::VerticalLcd => SmoothMode::VerticalLcd.into(),
            Self::Mono => Target::Mono,
        }
    }

    pub fn to_freetype_load_flags(self) -> LoadFlag {
        match self {
            Self::Normal => LoadFlag::TARGET_NORMAL,
            Self::Light => LoadFlag::TARGET_LIGHT,
            Self::Lcd => LoadFlag::TARGET_LCD,
            Self::VerticalLcd => LoadFlag::TARGET_LCD_V,
            Self::Mono => LoadFlag::TARGET_MONO,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Hinting {
    None,
    Interpreter(HintingTarget),
    Auto(HintingTarget),
}

impl Hinting {
    pub fn skrifa_options(self) -> Option<HintingOptions> {
        match self {
            Self::None => None,
            Self::Interpreter(target) => Some(HintingOptions {
                engine: ::skrifa::outline::Engine::Interpreter,
                target: target.to_skrifa_target(),
            }),
            Self::Auto(target) => Some(HintingOptions {
                engine: ::skrifa::outline::Engine::Auto(None),
                target: target.to_skrifa_target(),
            }),
        }
    }

    pub fn freetype_load_flags(self) -> LoadFlag {
        match self {
            Self::None => LoadFlag::NO_HINTING,
            Self::Interpreter(target) => LoadFlag::NO_AUTOHINT | target.to_freetype_load_flags(),
            Self::Auto(target) => LoadFlag::FORCE_AUTOHINT | target.to_freetype_load_flags(),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub struct InstanceOptions<'a> {
    pub index: usize,
    pub ppem: f32,
    pub coords: &'a [F2Dot14],
    pub hinting: Hinting,
}

impl<'a> InstanceOptions<'a> {
    pub fn new(index: usize, ppem: f32, coords: &'a [F2Dot14], hinting: Hinting) -> Self {
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
        let count = FontRef::fonts(data.0.as_ref()).count();
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
    ) -> Option<(FreeTypeInstance, SkrifaInstance<'_>)> {
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
