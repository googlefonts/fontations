#!/usr/bin/env python3
"""Generates the expected output files of skera's subset integration tests.

For every test in the given test suite files (default: all of tests/*.tests), this subsets the
test font with skera, checks that skera's output matches hb-subset's output for the same
arguments, and copies skera's output to expected/<test suite name>/. The integration test
compares skera's output to these files byte for byte.

Like the integration test, instances and IUP options are ignored, and the profiles
downgrade-cff2.txt and desubroutinize.txt are skipped. MATH tables are dropped, since skera
doesn't subset them yet.

Usage:
    generate-expected-outputs.py <hb-subset> <skera> [<test suite file> ...]

e.g. from the skera directory:
    cargo build --release -p skera --features cli
    test-data/generate-expected-outputs.py ~/harfbuzz/build/util/hb-subset \
        ../target/release/skera test-data/tests/basics.tests

Profiles with --retain-num-glyphs or --iftb-requirements need an hb-subset built with
-Dexperimental_api=true.
"""

import argparse
import concurrent.futures
import difflib
import io
import os
import re
import shutil
import subprocess
import sys
import tempfile

from fontTools.ttLib import TTFont

TEST_DATA_DIR = os.path.dirname(os.path.abspath(__file__))
# Not supported by skera yet; skipped like in the integration test.
SKIPPED_PROFILES = {"downgrade-cff2.txt", "desubroutinize.txt"}
# DSIG, which hb-subset's test runner also drops, and tables skera can't subset yet.
DROP_TABLES = "--drop-tables+=DSIG,MATH"
SECTIONS = ("FONTS:", "PROFILES:", "SUBSETS:", "INSTANCES:", "OPTIONS:", "IUP_OPTIONS:")


def parse_test_suite(path):
    """Returns the fonts, profiles and subsets of a test suite file."""
    sections = {name: [] for name in SECTIONS}
    current = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            if line in sections:
                current = line
            elif current is None:
                raise ValueError(f"{path}: unexpected line '{line}' before a heading")
            else:
                sections[current].append(line)
    profiles = [p for p in sections["PROFILES:"] if p not in SKIPPED_PROFILES]
    return sections["FONTS:"], profiles, sections["SUBSETS:"]


def subset_unicodes(subset):
    """Converts a SUBSETS: line to a --unicodes value, like the integration test."""
    if subset == "*":
        return "*"
    if subset == "no-unicodes":
        return ""
    if subset.startswith("U+"):
        return subset.replace("U+", "")
    return ",".join("%X" % ord(c) for c in subset.strip())


def subset_font_name(font, unicodes, profile):
    """The expected file name, like gen_subset_font_name() in the integration test."""
    subset_name = {"*": "all", "": "no-unicodes"}.get(unicodes, unicodes)
    font_base, font_ext = font.rsplit(".", 1)
    profile_name = profile.rsplit(".", 1)[0]
    return f"{font_base}.{profile_name}.{subset_name}.{font_ext}"


def ttx(path):
    """The ttx dump of a font, with checkSumAdjustment zeroed."""
    with io.StringIO() as fp:
        with TTFont(path) as font:
            font.saveXML(fp)
        return re.sub(
            'checkSumAdjustment value="0x[0-9a-fA-F]+"',
            'checkSumAdjustment value="0x00000000"',
            fp.getvalue(),
            count=1,
        )


def run(args):
    result = subprocess.run(args, capture_output=True, text=True)
    if result.returncode != 0:
        raise RuntimeError(f"{' '.join(args)}\nfailed:\n{result.stderr.strip()}")


def generate_one(hb_subset, skera, font, profile, subset, output_dir):
    """Generates one expected file. Returns an error message, or None on success."""
    font_path = os.path.join(TEST_DATA_DIR, "fonts", font)
    with open(os.path.join(TEST_DATA_DIR, "profiles", profile), encoding="utf-8") as f:
        profile_flags = [line.strip() for line in f if line.strip()]
    unicodes = subset_unicodes(subset)
    name = subset_font_name(font, unicodes, profile)

    with tempfile.TemporaryDirectory() as tmp:
        skera_out = os.path.join(tmp, "skera-" + name)
        hb_out = os.path.join(tmp, "hb-" + name)
        unicodes_flags = [f"--unicodes={unicodes}"] if unicodes else []
        try:
            run(
                [skera, f"--font-file={font_path}", f"--output-file={skera_out}"]
                + unicodes_flags
                + [DROP_TABLES]
                + profile_flags
            )
            run(
                [hb_subset, f"--font-file={font_path}", f"--output-file={hb_out}"]
                + unicodes_flags
                + [DROP_TABLES]
                + profile_flags
            )
        except RuntimeError as e:
            return f"{name}: {e}"

        skera_ttx, hb_ttx = ttx(skera_out), ttx(hb_out)
        if skera_ttx != hb_ttx:
            diff = "".join(
                difflib.unified_diff(
                    hb_ttx.splitlines(True),
                    skera_ttx.splitlines(True),
                    "hb-subset/" + name,
                    "skera/" + name,
                )
            )
            return f"{name}: ttx for hb-subset and skera does not match\n{diff}"

        shutil.copy(skera_out, os.path.join(output_dir, name))
    return None


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("hb_subset", help="path to the hb-subset executable")
    parser.add_argument("skera", help="path to the skera executable, built with --features cli")
    parser.add_argument(
        "test_suites",
        nargs="*",
        help="test suite files (default: all of tests/*.tests)",
    )
    parser.add_argument(
        "-j", "--jobs", type=int, default=os.cpu_count(), help="number of parallel jobs"
    )
    args = parser.parse_args()

    test_suites = args.test_suites or sorted(
        os.path.join(TEST_DATA_DIR, "tests", f)
        for f in os.listdir(os.path.join(TEST_DATA_DIR, "tests"))
        if f.endswith(".tests")
    )

    failures = []
    generated = 0
    with concurrent.futures.ProcessPoolExecutor(max_workers=args.jobs) as executor:
        for suite in test_suites:
            fonts, profiles, subsets = parse_test_suite(suite)
            suite_name = os.path.splitext(os.path.basename(suite))[0]
            output_dir = os.path.join(TEST_DATA_DIR, "expected", suite_name)
            os.makedirs(output_dir, exist_ok=True)
            print(f"Generating output files for {output_dir}", flush=True)

            futures = [
                executor.submit(
                    generate_one, args.hb_subset, args.skera, font, profile, subset, output_dir
                )
                for font in fonts
                for profile in profiles
                for subset in subsets
            ]
            for future in futures:
                error = future.result()
                if error:
                    failures.append(f"{suite_name}/{error}")
                else:
                    generated += 1

    for failure in failures:
        print(f"\nFAILED: {failure}", file=sys.stderr)
    print(f"\n{generated} expected files generated, {len(failures)} failed.")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
