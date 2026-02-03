use read_fonts::tables::gpos::{PositionLookup, SanitizedPositionLookup};
use read_fonts::tables::gsub::{SanitizedSubstitutionLookup, SubstitutionLookup};
use std::collections::HashMap;

use criterion::{criterion_group, criterion_main, Criterion};
use read_fonts::{FontRef, Sanitize, TableProvider};

pub fn sanitize_check_lookups(c: &mut Criterion) {
    eprintln!("{:?}", std::env::current_dir());
    let bytes = std::fs::read("../../harfrust/benches/fonts/NotoNastaliqUrdu-Regular.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();

    c.bench_function("sanitized", |b| {
        b.iter(|| {
            let gpos = font.gpos().unwrap().sanitize().unwrap();
            let lookuplist = gpos.lookup_list().unwrap();
            let mut counts = HashMap::new();
            let mut add = |key| {
                *counts.entry(key).or_insert(0usize) += 1;
            };
            for lookup in lookuplist.lookups().iter() {
                match lookup.unwrap() {
                    SanitizedPositionLookup::Single(lookup) => {
                        for hmm in lookup.subtables().iter() {
                            let hmm = hmm.unwrap();
                            add((1, hmm.pos_format()));
                        }
                    }
                    SanitizedPositionLookup::Pair(lookup) => lookup
                        .subtables()
                        .iter()
                        .for_each(|sub| add((2, sub.unwrap().pos_format()))),
                    SanitizedPositionLookup::Cursive(l) => {
                        l.subtables().iter().for_each(|_| add((3, 1)));
                    }
                    SanitizedPositionLookup::MarkToBase(l) => {
                        l.subtables().iter().for_each(|_| add((4, 1)))
                    }
                    SanitizedPositionLookup::MarkToLig(l) => {
                        l.subtables().iter().for_each(|_| add((5, 1)));
                    }
                    SanitizedPositionLookup::MarkToMark(l) => {
                        l.subtables().iter().for_each(|_| add((6, 1)));
                    }
                    SanitizedPositionLookup::Contextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((7, sub.unwrap().format()))),
                    SanitizedPositionLookup::ChainContextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((8, sub.unwrap().format()))),

                    SanitizedPositionLookup::Extension(l) => {
                        l.subtables().iter().for_each(|_| add((9, 1)))
                    }
                };
            }

            let gsub = font.gsub().unwrap().sanitize().unwrap();
            let lookuplist = gsub.lookup_list().unwrap();
            for lookup in lookuplist.lookups().iter() {
                match lookup.unwrap() {
                    SanitizedSubstitutionLookup::Single(lookup) => {
                        for sub in lookup.subtables().iter() {
                            add((1, sub.unwrap().subst_format()));
                        }
                    }
                    SanitizedSubstitutionLookup::Multiple(l) => {
                        l.subtables().iter().for_each(|_| add((2, 1)));
                    }
                    SanitizedSubstitutionLookup::Alternate(l) => {
                        l.subtables().iter().for_each(|_| add((3, 1)));
                    }
                    SanitizedSubstitutionLookup::Ligature(l) => {
                        l.subtables().iter().for_each(|_| add((4, 1)));
                    }
                    SanitizedSubstitutionLookup::Contextual(l) => {
                        l.subtables().iter().for_each(|_| add((5, 1)));
                    }
                    SanitizedSubstitutionLookup::ChainContextual(l) => {
                        l.subtables().iter().for_each(|_| add((6, 1)));
                    }
                    SanitizedSubstitutionLookup::Extension(l) => {
                        l.subtables().iter().for_each(|_| add((7, 1)))
                    }
                    SanitizedSubstitutionLookup::Reverse(l) => {
                        l.subtables().iter().for_each(|_| add((8, 1)));
                    }
                };
            }
        })
    });
}

pub fn plain_check_lookups(c: &mut Criterion) {
    let bytes = std::fs::read("../../harfrust/benches/fonts/NotoNastaliqUrdu-Regular.ttf").unwrap();
    let font = FontRef::new(&bytes).unwrap();

    c.bench_function("no sanitize", |b| {
        b.iter(|| {
            let gpos = font.gpos().unwrap();
            let lookuplist = gpos.lookup_list().unwrap();
            let mut counts = HashMap::new();
            let mut add = |key| {
                *counts.entry(key).or_insert(0usize) += 1;
            };
            for lookup in lookuplist.lookups().iter() {
                match lookup.unwrap() {
                    PositionLookup::Single(lookup) => {
                        for hmm in lookup.subtables().iter() {
                            let hmm = hmm.unwrap();
                            add((1, hmm.pos_format()));
                        }
                    }
                    PositionLookup::Pair(lookup) => lookup
                        .subtables()
                        .iter()
                        .for_each(|sub| add((2, sub.unwrap().pos_format()))),
                    PositionLookup::Cursive(l) => {
                        l.subtables().iter().for_each(|_| add((3, 1)));
                    }
                    PositionLookup::MarkToBase(l) => l.subtables().iter().for_each(|_| add((4, 1))),
                    PositionLookup::MarkToLig(l) => {
                        l.subtables().iter().for_each(|_| add((5, 1)));
                    }
                    PositionLookup::MarkToMark(l) => {
                        l.subtables().iter().for_each(|_| add((6, 1)));
                    }
                    PositionLookup::Contextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((7, sub.unwrap().format()))),
                    PositionLookup::ChainContextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((8, sub.unwrap().format()))),
                    PositionLookup::Extension(l) => l.subtables().iter().for_each(|_| add((9, 1))),
                };
            }

            let gsub = font.gsub().unwrap();
            let lookuplist = gsub.lookup_list().unwrap();
            for lookup in lookuplist.lookups().iter() {
                match lookup.unwrap() {
                    SubstitutionLookup::Single(lookup) => {
                        for sub in lookup.subtables().iter() {
                            add((1, sub.unwrap().subst_format()));
                        }
                    }
                    SubstitutionLookup::Multiple(l) => {
                        l.subtables().iter().for_each(|_| add((2, 1)));
                    }
                    SubstitutionLookup::Alternate(l) => {
                        l.subtables().iter().for_each(|_| add((3, 1)));
                    }
                    SubstitutionLookup::Ligature(l) => {
                        l.subtables().iter().for_each(|_| add((4, 1)));
                    }
                    SubstitutionLookup::Contextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((5, sub.unwrap().format()))),
                    SubstitutionLookup::ChainContextual(l) => l
                        .subtables()
                        .iter()
                        .for_each(|sub| add((6, sub.unwrap().format()))),
                    SubstitutionLookup::Extension(l) => {
                        l.subtables().iter().for_each(|_| add((7, 1)))
                    }
                    SubstitutionLookup::Reverse(l) => {
                        l.subtables().iter().for_each(|_| add((8, 1)));
                    }
                };
            }
        })
    });
}

//pub fn lookup_ordered_benchmark(c: &mut Criterion) {
//let inputs = set_parameters();

//for input in inputs {
//let set = random_set(input.set_size, input.max_value());
//let mut needle = input.max_value() / 2;
//c.bench_with_input(
//BenchmarkId::new("BM_SetLookup/ordered", &input),
//&set,
//|b, s: &IntSet<u32>| {
//b.iter(|| {
//needle += 3;
//s.contains(needle % input.max_value())
//})
//},
//);
//}
//}

criterion_group!(benches, sanitize_check_lookups, plain_check_lookups,);
criterion_main!(benches);
