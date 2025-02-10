//! A directory of all the font tables.

// NOTE: if you add a new table, also add it to the test below to make sure
// that serde works!

pub mod avar;
pub mod base;
pub mod cmap;
pub mod fvar;
pub mod gasp;
pub mod gdef;
pub mod glyf;
pub mod gpos;
pub mod gsub;
pub mod gvar;
pub mod head;
pub mod hhea;
pub mod hmtx;
pub mod hvar;
pub mod layout;
pub mod loca;
pub mod maxp;
pub mod meta;
pub mod mvar;
pub mod name;
pub mod os2;
pub mod post;
pub mod sbix;
pub mod stat;
pub mod variations;
pub mod vhea;
pub mod vmtx;

#[cfg(feature = "ift")]
pub mod ift;

// ensure that all of our types implement the serde traits
#[cfg(feature = "serde")]
#[test]
fn do_we_even_serde() {
    #[derive(Default, serde::Deserialize, serde::Serialize)]
    struct AllTables {
        avar: avar::Avar,
        base: base::Base,
        cmap: cmap::Cmap,
        fvar: fvar::Fvar,
        gasp: gasp::Gasp,
        gdef: gdef::Gdef,
        glyf: glyf::Glyf,
        gpos: gpos::Gpos,
        gsub: gsub::Gsub,
        gvar: gvar::Gvar,
        head: head::Head,
        hhea: hhea::Hhea,
        hmtx: hmtx::Hmtx,
        hvar: hvar::Hvar,
        loca: loca::Loca,
        maxp: maxp::Maxp,
        meta: meta::Meta,
        name: name::Name,
        os2: os2::Os2,
        post: post::Post,
        sbix: sbix::Sbix,
        stat: stat::Stat,
        vhea: vhea::Vhea,
        vmtx: vmtx::Vmtx,
        ift: ift::Ift,
    }
    let tables = AllTables::default();
    let dumped = bincode::serialize(&tables).unwrap();
    let _loaded: AllTables = bincode::deserialize(&dumped).unwrap();
}
