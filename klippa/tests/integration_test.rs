//! Test subsetter output against expected results.
//!
//! This reads test configs from harfbuzz subset test suite,
//! generate a group of tests to perform, run and then compare the output against the stored expected result
//!
//! To generate the expected output files, pass GEN_EXPECTED_OUTPUTS=1 as an
//! environment variable.

use klippa::{parse_unicodes, subset_font, Plan};
use std::fmt::Write;
use std::fs;
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempdir::TempDir;
use write_fonts::read::{collections::IntSet, FontRef};

static TEST_DATA_DIR: &str = "./test-data";
static GEN_EXPECTED_OUTPUTS_VAR: &str = "GEN_EXPECTED_OUTPUTS";

#[derive(Default)]
struct SubsetTestCase {
    /// name of directory that stores expected files
    expected_dir: String,

    /// Path to font files used for testing
    fonts: Vec<String>,

    /// command line args for subsetter
    profiles: Vec<String>,

    ///subset codepoints to retain
    subsets: Vec<String>,

    ///command line args for instancer
    //TODO: add support for instancing
    //instances: Vec<String>,

    ///compare against fonttools or not
    fonttool_options: bool,

    ///IUP optimize or not
    iup_optimize: Vec<bool>,
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

    fn parse_profiles(&mut self, lines: &mut LinesIter) {
        while !lines.is_end() {
            if let Some(next) = lines.next() {
                self.case.profiles.push(next.trim().to_owned());
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
        //TODO: add support for instancing
        while !lines.is_end() {
            lines.next();
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

impl SubsetTestCase {
    fn new(path: &Path) -> Self {
        let parser = TestCaseParser::new();
        parser.parse(path)
    }

    fn run(&self) {
        let output_temp_dir = TempDir::new_in(".", "klippa_test").unwrap();
        let output_dir = output_temp_dir.path();
        for font in &self.fonts {
            //TODO: add support for profiles later
            //for profile in &self.profiles {
            //}
            for subset in &self.subsets {
                //TODO: add support for instances/iup_options
                self.run_one_test(font, subset, output_dir);
            }
        }
    }

    fn gen_expected_output(&self) {
        let output_temp_dir = TempDir::new_in(".", "klippa_test").unwrap();
        let output_dir = output_temp_dir.path();
        for font in &self.fonts {
            //TODO: add support for profiles later
            //for profile in &self.profiles {
            //}
            for subset in &self.subsets {
                //TODO: add support for instances/iup_options
                self.gen_expected_output_for_one_test(font, subset, output_dir);
            }
        }
        let expected_dir = Path::new(TEST_DATA_DIR)
            .join("expected")
            .join(&self.expected_dir);
        fs::rename(output_dir, expected_dir).unwrap();
    }

    fn run_one_test(&self, font: &str, subset: &str, output_dir: &Path) {
        //TODO: re-enable subset="*" once populate_unicodes_to_retain supports *
        if subset == "*" {
            return;
        }
        let subset_font_name = gen_subset_font_name(font, subset);
        let output_file = output_dir.join(&subset_font_name);
        gen_subset_font_file(font, subset, &output_file);

        let expected_file = Path::new(TEST_DATA_DIR)
            .join("expected")
            .join(&self.expected_dir)
            .join(&subset_font_name);
        compare_with_expected(output_dir, &output_file, &expected_file);
    }

    fn gen_expected_output_for_one_test(&self, font: &str, subset: &str, output_dir: &Path) {
        //TODO: re-enable subset="*" once populate_unicodes_to_retain supports *
        if subset == "*" {
            return;
        }

        let subset_font_name = gen_subset_font_name(font, subset);
        let output_file = output_dir.join(&subset_font_name);
        gen_subset_font_file(font, subset, &output_file);

        assert_has_ttx_exec();
        let mut expected_file_name = String::from(&subset_font_name);
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
            .arg("--drop-tables+=DSIG,BASE")
            .arg("--drop-tables-=sbix")
            .arg("--no-harfbuzz-repacker")
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
            panic!("{diff}\nError: ttx for fonttools and klippa does not match.");
        }
        fs::remove_file(expected_file).unwrap();
        fs::remove_file(expected_ttx).unwrap();
        fs::remove_file(output_ttx).unwrap();
    }
}

fn gen_subset_font_file(font_file: &str, subset: &str, output_file: &PathBuf) {
    let org_font_file = PathBuf::from(TEST_DATA_DIR).join("fonts").join(font_file);
    let org_font_bytes = std::fs::read(org_font_file).unwrap();
    let font = FontRef::new(&org_font_bytes).unwrap();

    let gids = IntSet::empty();
    let unicodes = parse_unicodes(subset).unwrap();
    let plan = Plan::new(&gids, &unicodes, &font);

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
            write!(&mut out, "{:x}", c).unwrap();
        } else {
            write!(&mut out, ",{:x}", c).unwrap();
        }
    }
    out
}

fn strip_unicode_prefix(text: &str) -> String {
    text.replace("U+", "")
}

fn gen_subset_font_name(font: &str, subset: &str) -> String {
    let subset_name = match subset {
        "*" => "all",
        "" => "no-unicodes",
        _ => subset,
    };

    let (font_base_name, font_extension) = font.rsplit_once('.').unwrap();
    //TODO: add profiles/instances later
    let subset_font_name = format!("{font_base_name}.{subset_name}.{font_extension}");
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

fn write_lines(f: &mut impl Write, lines: &[&str], line_num: usize, prefix: char) {
    writeln!(f, "L{}", line_num).unwrap();
    for line in lines {
        writeln!(f, "{}  {}", prefix, line).unwrap();
    }
}

fn diff_ttx(expected_ttx: &Path, output_ttx: &Path) -> String {
    let expected = fs::read_to_string(expected_ttx).unwrap();
    let output = fs::read_to_string(output_ttx).unwrap();
    let lines = diff::lines(&expected, &output);

    let mut result = String::new();
    let mut temp: Vec<&str> = Vec::new();
    let mut left_or_right = None;
    let mut section_start = 0;

    for (i, line) in lines.iter().enumerate() {
        match line {
            diff::Result::Left(line) => {
                if line.contains("checkSumAdjustment value=") {
                    continue;
                }
                if left_or_right == Some('R') {
                    write_lines(&mut result, &temp, section_start, '<');
                    temp.clear();
                } else if left_or_right != Some('L') {
                    section_start = i;
                }
                temp.push(line);
                left_or_right = Some('L');
            }
            diff::Result::Right(line) => {
                if line.contains("checkSumAdjustment value=") {
                    continue;
                }
                if left_or_right == Some('L') {
                    write_lines(&mut result, &temp, section_start, '>');
                    temp.clear();
                } else if left_or_right != Some('R') {
                    section_start = i;
                }
                temp.push(line);
                left_or_right = Some('R');
            }
            diff::Result::Both { .. } => {
                match left_or_right.take() {
                    Some('R') => write_lines(&mut result, &temp, section_start, '<'),
                    Some('L') => write_lines(&mut result, &temp, section_start, '>'),
                    _ => (),
                }
                temp.clear();
            }
        }
    }
    match left_or_right.take() {
        Some('R') => write_lines(&mut result, &temp, section_start, '<'),
        Some('L') => write_lines(&mut result, &temp, section_start, '>'),
        _ => (),
    }
    result
}

fn compare_with_expected(output_dir: &Path, output_file: &Path, expected_file: &Path) {
    let expected = fs::read(expected_file).unwrap();
    let output = fs::read(output_file).unwrap();
    if expected != output {
        assert_has_ttx_exec();
        let expected_file_prefix = expected_file.file_stem().unwrap().to_str().unwrap();
        let expected_ttx = format!("{expected_file_prefix}.expected.ttx");
        let expected_ttx = output_dir.join(expected_ttx);
        Command::new("ttx")
            .arg("-o")
            .arg(&expected_ttx)
            .arg(expected_file)
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

        let ttx_diff = diff_ttx(&expected_ttx, &output_ttx);
        //TODO: print more info about the test state
        panic!("{ttx_diff}\nError: ttx for expected and actual does not match.");
    }
}

#[test]
fn run_all_tests() {
    use std::ffi::OsStr;
    let tests_path = Path::new(TEST_DATA_DIR).join("tests");
    for entry in tests_path.read_dir().expect("can't read dir: test-data") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension() == Some(OsStr::new("tests")) {
            let test = SubsetTestCase::new(&path);
            match std::env::var(GEN_EXPECTED_OUTPUTS_VAR) {
                Ok(_val) => {
                    test.gen_expected_output();
                }
                Err(_e) => {
                    test.run();
                }
            }
        }
    }
}

#[test]
fn parse_test() {
    let test_data_dir = Path::new(TEST_DATA_DIR);
    assert!(test_data_dir.exists());
    let test_file = test_data_dir.join("tests/basics.tests");
    let subset_test = SubsetTestCase::new(&test_file);
    assert_eq!(subset_test.fonts.len(), 1);
    assert_eq!(subset_test.fonts[0], "Roboto-Regular.abc.ttf");
    assert_eq!(subset_test.profiles.len(), 13);
    assert_eq!(subset_test.subsets.len(), 5);
    assert_eq!(subset_test.subsets[0], "61,62,63");
}
