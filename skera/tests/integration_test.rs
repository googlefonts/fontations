//! Test subsetter output against expected results.
//!
//! This reads test configs from harfbuzz subset test suite,
//! generate a group of tests to perform, run and then compare the output against the stored expected result
//!
//! To generate the expected output files, pass GEN_EXPECTED_OUTPUTS=1 as an
//! environment variable.

use libtest_mimic::{Arguments, Trial};
use similar::TextDiff;
use skera::{parse_unicodes, subset_font, Plan, SubsetFlags, DEFAULT_LAYOUT_FEATURES};
use skrifa::GlyphId;
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    fs,
    iter::Peekable,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tempfile::Builder;
use write_fonts::{
    read::{collections::IntSet, FontRef},
    types::{NameId, Tag},
};

static TEST_DATA_DIR: &str = "./test-data";
static GEN_EXPECTED_OUTPUTS_VAR: &str = "GEN_EXPECTED_OUTPUTS";

#[derive(Default)]
struct SubsetTestCase {
    /// name of directory that stores expected files
    expected_dir: String,

    /// Path to font files used for testing
    fonts: Vec<String>,

    /// command line args for subsetter
    profiles: Vec<(String, SubsetInput)>,

    /// subset codepoints to retain
    subsets: Vec<String>,

    /// command line args for instancer
    instances: Vec<String>,
    ///compare against fonttools or not
    fonttool_options: bool,

    ///IUP optimize or not
    iup_optimize: Vec<bool>,
}

#[derive(Default, Clone)]
struct SubsetInput {
    pub subset_flag: SubsetFlags,
    pub name_ids: IntSet<NameId>,
    pub name_languages: IntSet<u16>,
    pub gids: IntSet<GlyphId>,
    pub layout_scripts: IntSet<Tag>,
    pub layout_features: IntSet<Tag>,
}

#[derive(Default)]
struct TestCaseParser {
    case: SubsetTestCase,
}

/// A helper to iterate over non-empty lines of input
struct LinesIter<'a> {
    iter: Peekable<std::str::Lines<'a>>,
}

impl<'a> LinesIter<'a> {
    fn new(s: &'a str) -> Self {
        let mut this = Self {
            iter: s.lines().peekable(),
        };
        this.skip_empty_lines();
        this
    }

    fn next(&mut self) -> Option<&'a str> {
        let next = self.iter.next();
        self.skip_empty_lines();
        next
    }

    fn skip_empty_lines(&mut self) {
        while let Some(next) = self.iter.peek().copied() {
            let next = next.trim();
            if !(next.starts_with('#') || next.is_empty()) {
                break;
            }
            self.iter.next();
        }
    }

    fn is_end(&mut self) -> bool {
        matches!(
            self.iter.peek().copied(),
            None | Some(
                "FONTS:" | "PROFILES:" | "SUBSETS:" | "INSTANCES:" | "OPTIONS:" | "IUP_OPTIONS:",
            )
        )
    }
}

impl TestCaseParser {
    fn new() -> Self {
        TestCaseParser::default()
    }

    fn parse(mut self, path: &Path) -> SubsetTestCase {
        self.case.expected_dir = String::from(path.file_stem().unwrap().to_str().unwrap());

        let input_text = std::fs::read_to_string(path).unwrap();
        let mut lines = LinesIter::new(&input_text);
        while let Some(line) = lines.next() {
            match line {
                "FONTS:" => self.parse_fonts(&mut lines),
                "PROFILES:" => self.parse_profiles(&mut lines),
                "SUBSETS:" => self.parse_subsets(&mut lines),
                "INSTANCES:" => self.parse_instances(&mut lines),
                "OPTIONS:" => self.parse_fonttools_options(&mut lines),
                "IUP_OPTIONS:" => self.parse_iup_options(&mut lines),
                other => panic!("unexpected heading '{other}'"),
            }
        }
        self.case
    }

    fn parse_fonts(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                self.case.fonts.push(next.trim().to_owned());
            }
        }
    }

    //TODO: when we support more options that are not just subset flags, make profiles to be Vec<(String, SubsetInput)>
    fn parse_profiles(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                let subset_input = parse_profile_options(next.trim());
                self.case
                    .profiles
                    .push((next.trim().to_owned(), subset_input));
            }
        }
    }

    fn parse_subsets(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                match next {
                    "*" => self.case.subsets.push(next.to_owned()),
                    "no-unicodes" => self.case.subsets.push(String::new()),
                    // unicode string
                    next if next.starts_with("U+") => {
                        self.case.subsets.push(strip_unicode_prefix(next))
                    }
                    //convert text string to unicode string
                    _ => self.case.subsets.push(convert_text_to_unicodes(next)),
                }
            }
        }
    }

    fn parse_instances(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                self.case.instances.push(next.trim().to_owned());
            }
        }
    }

    fn parse_fonttools_options(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                match next {
                    "no_fonttools" => {
                        self.case.fonttool_options = true;
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
    }

    fn parse_iup_options(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                match next {
                    "Yes" => {
                        self.case.iup_optimize.push(true);
                    }
                    "No" => {
                        self.case.iup_optimize.push(false);
                    }
                    _ => {
                        continue;
                    }
                }
            }
        }
    }
}

fn parse_profile_options(file_name: &str) -> SubsetInput {
    let file_path = Path::new(TEST_DATA_DIR).join("profiles").join(file_name);
    let input = std::fs::read_to_string(file_path).unwrap();
    let mut subset_flag = SubsetFlags::SUBSET_FLAGS_DEFAULT;
    let mut name_ids = IntSet::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));

    let mut name_languages = IntSet::<u16>::empty();
    name_languages.insert(0x0409);

    let mut gids = IntSet::empty();

    let mut layout_scripts = IntSet::<Tag>::empty();
    layout_scripts.invert();

    let mut layout_features = IntSet::<Tag>::empty();
    layout_features.extend(DEFAULT_LAYOUT_FEATURES.iter().copied());

    //TODO: parse str instead of hard code
    for line in input.lines() {
        match line.trim() {
            "--desubroutinize" => subset_flag |= SubsetFlags::SUBSET_FLAGS_DESUBROUTINIZE,
            "--retain-gids" => subset_flag |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS,
            "--no-hinting" => subset_flag |= SubsetFlags::SUBSET_FLAGS_NO_HINTING,
            "--glyph-names" => subset_flag |= SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES,
            "--name-legacy" => subset_flag |= SubsetFlags::SUBSET_FLAGS_NAME_LEGACY,
            "--no-layout-closure" => subset_flag |= SubsetFlags::SUBSET_FLAGS_NO_LAYOUT_CLOSURE,
            "--no-prune-unicode-ranges" => {
                subset_flag |= SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES
            }
            "--notdef-outline" => subset_flag |= SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE,
            "--name-IDs=0,1,2" => {
                name_ids.clear();
                name_ids.insert_range(NameId::from(0)..=NameId::from(2));
            }
            "--name-languages=*" => {
                name_languages.clear();
                name_languages.invert();
            }
            "--gids=1,2,3" => {
                gids.insert_range(GlyphId::new(1)..=GlyphId::new(3));
            }
            "--layout-scripts=grek,latn" => {
                layout_scripts.clear();
                layout_scripts.insert(Tag::new(b"grek"));
                layout_scripts.insert(Tag::new(b"latn"));
            }
            "--layout-scripts=grek,cyrl" => {
                layout_scripts.clear();
                layout_scripts.insert(Tag::new(b"grek"));
                layout_scripts.insert(Tag::new(b"cyrl"));
            }
            "--layout-scripts-=*" => {
                layout_scripts.clear();
            }
            _ => continue,
        }
    }
    SubsetInput {
        subset_flag,
        name_ids,
        name_languages,
        gids,
        layout_scripts,
        layout_features,
    }
}

struct IndividualTestCase {
    font: String,
    subset: String,
    profile: (String, SubsetInput),
    instance: Option<String>,
    expected_dir: String,
}

impl IndividualTestCase {
    fn name(&self) -> PathBuf {
        gen_subset_font_name(
            &self.font,
            &self.subset,
            self.profile.0.as_str(),
            self.instance.as_deref(),
        )
    }
    fn run(&self, output_dir: &Path) {
        let subset_font_name = self.name();
        let output_file = output_dir.join(&subset_font_name);
        gen_subset_font_file(
            &self.font,
            &self.subset,
            &self.profile.1,
            self.instance.as_deref(),
            &output_file,
        );

        let expected_file = Path::new(TEST_DATA_DIR)
            .join("expected")
            .join(&self.expected_dir)
            .join(&subset_font_name);
        compare_with_expected(output_dir, &output_file, &expected_file);
    }
}

impl SubsetTestCase {
    fn new(path: &Path) -> Self {
        let parser = TestCaseParser::new();
        parser.parse(path)
    }

    fn collect_subtests(&self) -> Vec<IndividualTestCase> {
        let mut subtests = vec![];
        for font in &self.fonts {
            if font.ends_with(".otf") {
                continue;
            }
            for profile in &self.profiles {
                for subset in &self.subsets {
                    if self.instances.is_empty() {
                        subtests.push(IndividualTestCase {
                            font: font.clone(),
                            subset: subset.clone(),
                            profile: (profile.0.clone(), profile.1.clone()),
                            instance: None,
                            expected_dir: self.expected_dir.clone(),
                        });
                    } else {
                        for instance in &self.instances {
                            subtests.push(IndividualTestCase {
                                font: font.clone(),
                                subset: subset.clone(),
                                profile: (profile.0.clone(), profile.1.clone()),
                                instance: Some(instance.clone()),
                                expected_dir: self.expected_dir.clone(),
                            });
                        }
                    }
                }
            }
        }
        subtests
    }

    fn gen_expected_output(&self) {
        let output_temp_dir = Builder::new().prefix("skera_test").tempdir_in(".").unwrap();
        let output_dir = output_temp_dir.into_path();
        for font in &self.fonts {
            for profile in &self.profiles {
                for subset in &self.subsets {
                    if self.instances.is_empty() {
                        self.gen_expected_output_for_one_test(
                            font,
                            subset,
                            profile,
                            None,
                            &output_dir,
                        );
                    } else {
                        for instance in &self.instances {
                            self.gen_expected_output_for_one_test(
                                font,
                                subset,
                                profile,
                                Some(instance.as_str()),
                                &output_dir,
                            );
                        }
                    }
                }
            }
        }
        let expected_dir = Path::new(TEST_DATA_DIR)
            .join("expected")
            .join(&self.expected_dir);
        fs::rename(output_dir, expected_dir).unwrap();
    }

    fn gen_expected_output_for_one_test(
        &self,
        font: &str,
        subset: &str,
        profile: &(String, SubsetInput),
        instance: Option<&str>,
        output_dir: &Path,
    ) {
        let subset_font_name = gen_subset_font_name(font, subset, profile.0.as_str(), instance);
        let output_file = output_dir.join(&subset_font_name);
        gen_subset_font_file(font, subset, &profile.1, instance, &output_file);

        assert_has_ttx_exec();
        let mut expected_file_name = subset_font_name.to_str().unwrap().to_owned();
        expected_file_name.push_str(".expected");
        let expected_file = output_dir.join(expected_file_name);

        let mut unicodes_option = String::from("--unicodes=");
        unicodes_option.push_str(subset);

        let mut output_option = String::from("--output-file=");
        output_option.push_str(expected_file.to_str().unwrap());

        let org_font_file = Path::new(TEST_DATA_DIR).join("fonts").join(font);

        Command::new("fonttools")
            .arg("subset")
            .arg(&org_font_file)
            .arg("--drop-tables+=DSIG,fpgm,prep,cvt,gasp,cvar,STAT")
            .arg("--drop-tables-=sbix")
            .arg("--no-harfbuzz-repacker")
            .arg("--no-prune-codepage-ranges")
            .arg(&unicodes_option)
            .arg(output_option)
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .expect("fonttools failed to subset {org_font_file}");

        let expected_ttx = expected_file.with_extension("ttx");
        Command::new("ttx")
            .arg("-o")
            .arg(&expected_ttx)
            .arg(&expected_file)
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .expect("ttx failed to parse the expected file {expected_file}");

        let output_ttx = output_file.with_extension("ttx");
        Command::new("ttx")
            .arg("-o")
            .arg(&output_ttx)
            .arg(output_file)
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .expect("ttx failed to parse the output file {output_file}");

        let diff = diff_ttx(&expected_ttx, &output_ttx);
        if !diff.is_empty() {
            panic!("{diff}\nError: ttx for fonttools and skera does not match.");
        }
        fs::remove_file(expected_file).unwrap();
        fs::remove_file(expected_ttx).unwrap();
        fs::remove_file(output_ttx).unwrap();
    }
}

fn gen_subset_font_file(
    font_file: &str,
    subset: &str,
    profile: &SubsetInput,
    instance: Option<&str>,
    output_file: &PathBuf,
) {
    use skera::parse_instancing_spec;

    let org_font_file = PathBuf::from(TEST_DATA_DIR).join("fonts").join(font_file);
    let org_font_bytes = std::fs::read(org_font_file).unwrap();
    let font = FontRef::new(&org_font_bytes).unwrap();

    let unicodes = parse_unicodes(subset).unwrap();
    let drop_tables_str = "morx,mort,kerx,kern,JSTF,DSIG,EBDT,EBLC,EBSC,SVG,PCLT,LTSH,feat,Glat,Gloc,Silf,Sill,fpgm,prep,cvt,gasp,cvar,STAT";
    let mut drop_tables = IntSet::empty();
    for str in drop_tables_str.split(',') {
        let tag = Tag::new_checked(str.as_bytes()).unwrap();
        drop_tables.insert(tag);
    }

    let mut name_ids = IntSet::<NameId>::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    let mut name_languages = IntSet::<u16>::empty();
    name_languages.insert(0x0409);

    let instancing_spec = instance.and_then(|inst| parse_instancing_spec(inst).ok());

    let plan = Plan::new(
        &profile.gids,
        &unicodes,
        &font,
        profile.subset_flag,
        &drop_tables,
        &profile.layout_scripts,
        &profile.layout_features,
        &profile.name_ids,
        &profile.name_languages,
        &instancing_spec,
    );

    let subset_output = subset_font(&font, &plan).unwrap();
    std::fs::write(output_file, subset_output).unwrap();
    //TODO: re-enable OTS check
    //assert_has_ots_exec();
    //assert_check_ots(&output_file);
}

fn convert_text_to_unicodes(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = c as u32;
        if out.is_empty() {
            write!(&mut out, "{:X}", c).unwrap();
        } else {
            write!(&mut out, ",{:X}", c).unwrap();
        }
    }
    out
}

fn strip_unicode_prefix(text: &str) -> String {
    text.replace("U+", "")
}

fn gen_subset_font_name(
    font: &str,
    subset: &str,
    profile: &str,
    instance: Option<&str>,
) -> PathBuf {
    let subset_name = match subset {
        "*" => "all",
        "" => "no-unicodes",
        _ => subset,
    };

    let (font_base_name, font_extension) = font.rsplit_once('.').unwrap();
    let (profile_name, _profile_extension) = profile.rsplit_once('.').unwrap();

    let subset_font_name = if let Some(inst) = instance {
        let instance_name = inst.replace(':', "-");
        PathBuf::from(format!(
            "{font_base_name}.{profile_name}.{subset_name}.{instance_name}.{font_extension}"
        ))
    } else {
        PathBuf::from(format!(
            "{font_base_name}.{profile_name}.{subset_name}.{font_extension}"
        ))
    };
    subset_font_name
}
/// Assert that we can find the `ttx` executable
#[allow(dead_code)]
fn assert_has_ttx_exec() {
    assert!(
        Command::new("ttx")
            .arg("--version")
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "\nmissing `ttx` executable. Install it with `pip install fonttools`."
    )
}

/// Assert that we can find the `ots-sanitze` executable
#[allow(dead_code)]
fn assert_has_ots_exec() {
    assert!(
        Command::new("ots-sanitize")
            .arg("--version")
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "\nmissing `ots-sanitize` executable."
    )
}

#[allow(dead_code)]
fn assert_check_ots(file: &Path) {
    let file_name_str = file.to_str().unwrap();
    assert!(
        Command::new("ots-sanitize")
            .arg(file_name_str)
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false),
        "\nOTS failure on {file_name_str}!"
    )
}

fn diff_ttx(expected_ttx: &Path, output_ttx: &Path) -> String {
    let expected = fs::read_to_string(expected_ttx).unwrap();
    let output = fs::read_to_string(output_ttx).unwrap();
    let expected_per_table: HashMap<String, Vec<String>> = split_into_tables(&expected);
    let output_per_table: HashMap<String, Vec<String>> = split_into_tables(&output);
    let all_tables = expected_per_table
        .keys()
        .chain(output_per_table.keys())
        .collect::<HashSet<_>>();
    let mut result = String::new();
    for table in all_tables {
        match (expected_per_table.get(table), output_per_table.get(table)) {
            (Some(expected_lines), Some(output_lines)) => {
                if expected_lines != output_lines {
                    result += &(format!("\nDifference found in table '{table}':\n")
                        + &TextDiff::from_lines(
                            &expected_lines.join("\n"),
                            &output_lines.join("\n"),
                        )
                        .unified_diff()
                        .header("Expected", "Output")
                        .to_string()
                        + "\n\n");
                }
            }
            (Some(_), None) => {
                result += &format!("Output did not contain table {table}\n");
            }
            (None, Some(output_lines)) => {
                result += &format!("Output contained extraneous table {table}\n",);
            }
            (None, None) => unreachable!(),
        }
    }
    result
}

fn split_into_tables(output: &str) -> HashMap<String, Vec<String>> {
    let mut current_table = None;
    let mut hashmap: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines() {
        if line.contains("checkSumAdjustment") {
            continue;
        }
        if let Some(table_name) = line.strip_prefix("  <") {
            if table_name.starts_with('/') {
                current_table = None;
            } else {
                current_table = Some(table_name.trim_end_matches('>'));
            }
        } else if let Some(table_name) = current_table {
            hashmap
                .entry(table_name.to_owned())
                .or_default()
                .push(line.to_owned());
        }
    }
    hashmap
}

fn exclude_expected_failures(c: &mut Command) -> &mut Command {
    c.arg("-x")
        .arg("cvt ")
        .arg("-x")
        .arg("gasp")
        .arg("-x")
        .arg("prep")
        .arg("-x")
        .arg("fpgm")
        .arg("-x")
        .arg("FFTM")
}

fn compare_with_expected(output_dir: &Path, output_file: &Path, expected_file: &Path) {
    let expected = fs::read(expected_file).unwrap();
    let output = fs::read(output_file).unwrap();
    if expected != output {
        // uncomment to overwrite expected file with output for updating integration tests
        // fs::write(expected_file, &output).unwrap(); return;
        assert_has_ttx_exec();
        let expected_file_prefix = expected_file.file_stem().unwrap().to_str().unwrap();
        let expected_ttx = format!("{expected_file_prefix}.expected.ttx");
        let expected_ttx = output_dir.join(expected_ttx);
        exclude_expected_failures(
            Command::new("ttx")
                .arg("-o")
                .arg(&expected_ttx)
                .arg(expected_file),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .expect("ttx failed to parse the expected file {expected_file}");

        let output_ttx = output_file.with_extension("ttx");
        exclude_expected_failures(
            Command::new("ttx")
                .arg("-o")
                .arg(&output_ttx)
                .arg(output_file),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .expect("ttx failed to parse the output file {output_file}");

        let ttx_diff = diff_ttx(&expected_ttx, &output_ttx);
        if ttx_diff.trim_ascii().is_empty() {
            return;
        }
        //TODO: print more info about the test state
        panic!(
            "failed on {expected_file:?}\n{ttx_diff}\nError: ttx for expected and actual does not match."
        );
    }
}

fn test_cases() -> impl Iterator<Item = (String, SubsetTestCase)> {
    use std::ffi::OsStr;
    let tests_path = Path::new(TEST_DATA_DIR).join("tests");
    tests_path
        .read_dir()
        .expect("can't read dir: test-data")
        .flat_map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            let name = path
                .with_extension("")
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            if path.extension() == Some(OsStr::new("tests")) {
                Some((name, SubsetTestCase::new(&path)))
            } else {
                None
            }
        })
}

fn regression_tests() -> Vec<Trial> {
    let all_subtests: Vec<_> = test_cases()
        .map(|(category, test)| (category, test.collect_subtests()))
        .collect();
    let mut tests = vec![];
    for (category, subtests) in all_subtests {
        for test in subtests {
            let name = test.name();
            tests.push(Trial::test(
                category.clone() + "-" + name.file_name().unwrap().to_str().unwrap(),
                move || {
                    let output_temp_dir =
                        Builder::new().prefix("skera_test").tempdir_in(".").unwrap();
                    let output_dir = output_temp_dir.path();
                    test.run(output_dir);
                    Ok(())
                },
            ));
        }
    }
    tests
}

fn main() {
    let gen_expected_outputs = std::env::var(GEN_EXPECTED_OUTPUTS_VAR).is_ok();
    let args = Arguments::from_args();
    if gen_expected_outputs {
        for (name, test) in test_cases() {
            println!("generating expected output for {name}");
            test.gen_expected_output();
        }
    } else {
        let mut tests = regression_tests();

        let conclusion = libtest_mimic::run(&args, tests);
        conclusion.exit();
    }
}

#[test]
fn parse_test() {
    let test_data_dir = Path::new(TEST_DATA_DIR);
    assert!(test_data_dir.exists());
    let test_file = test_data_dir.join("tests/basics.tests");
    let subset_test = SubsetTestCase::new(&test_file);

    assert_eq!(subset_test.fonts.len(), 2);
    assert_eq!(subset_test.fonts[0], "Roboto-Regular.abc.ttf");
    assert_eq!(subset_test.profiles.len(), 12);
    assert_eq!(subset_test.profiles[0].0, String::from("default.txt"));
    assert_eq!(
        subset_test.profiles[0].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_DEFAULT
    );
    assert_eq!(subset_test.profiles[0].1.name_ids.len(), 7);
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(0)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(1)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(2)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(3)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(4)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(5)));
    assert!(subset_test.profiles[0].1.name_ids.contains(NameId::new(6)));

    assert_eq!(subset_test.profiles[0].1.name_languages.len(), 1);
    assert!(subset_test.profiles[0].1.name_languages.contains(0x409));

    assert!(subset_test.profiles[0].1.gids.is_empty());

    assert_eq!(subset_test.profiles[1].0, String::from("drop-hints.txt"));
    assert_eq!(
        subset_test.profiles[1].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING
    );

    assert_eq!(
        subset_test.profiles[2].0,
        String::from("drop-hints-retain-gids.txt")
    );
    assert_eq!(
        subset_test.profiles[2].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING | SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
    );

    assert_eq!(subset_test.profiles[3].0, String::from("retain-gids.txt"));
    assert_eq!(
        subset_test.profiles[3].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS
    );

    assert_eq!(
        subset_test.profiles[4].0,
        String::from("notdef-outline.txt")
    );
    assert_eq!(
        subset_test.profiles[4].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_NOTDEF_OUTLINE
    );

    assert_eq!(subset_test.profiles[5].0, String::from("name-ids.txt"));
    assert_eq!(
        subset_test.profiles[5].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_DEFAULT
    );
    assert_eq!(subset_test.profiles[5].1.name_ids.len(), 3);
    assert!(subset_test.profiles[5].1.name_ids.contains(NameId::new(0)));
    assert!(subset_test.profiles[5].1.name_ids.contains(NameId::new(1)));
    assert!(subset_test.profiles[5].1.name_ids.contains(NameId::new(2)));

    assert_eq!(
        subset_test.profiles[6].0,
        String::from("name-languages.txt")
    );
    assert_eq!(
        subset_test.profiles[6].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_DEFAULT
    );
    assert!(subset_test.profiles[6].1.name_languages.contains(1));
    assert!(subset_test.profiles[6].1.name_languages.contains(2));
    assert!(subset_test.profiles[6].1.name_languages.contains(3));

    assert_eq!(subset_test.profiles[7].0, String::from("name-legacy.txt"));
    assert_eq!(
        subset_test.profiles[7].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_NAME_LEGACY
    );

    assert_eq!(subset_test.profiles[8].0, String::from("gids.txt"));
    assert_eq!(
        subset_test.profiles[8].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_DEFAULT
    );
    assert_eq!(subset_test.profiles[8].1.gids.len(), 3);
    assert!(subset_test.profiles[8].1.gids.contains(GlyphId::new(1)));
    assert!(subset_test.profiles[8].1.gids.contains(GlyphId::new(2)));
    assert!(subset_test.profiles[8].1.gids.contains(GlyphId::new(3)));

    assert_eq!(
        subset_test.profiles[9].0,
        String::from("no-prune-unicode-ranges.txt")
    );
    assert_eq!(
        subset_test.profiles[9].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_NO_PRUNE_UNICODE_RANGES
    );

    assert_eq!(subset_test.profiles[10].0, String::from("glyph-names.txt"));
    assert_eq!(
        subset_test.profiles[10].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES
    );

    assert_eq!(
        subset_test.profiles[11].0,
        String::from("retain-gids-glyph-names.txt")
    );
    assert_eq!(
        subset_test.profiles[11].1.subset_flag,
        SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS | SubsetFlags::SUBSET_FLAGS_GLYPH_NAMES
    );
    assert_eq!(subset_test.subsets.len(), 3);
    assert_eq!(subset_test.subsets[1], "61,62,63");
}
