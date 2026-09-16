use std::fs;
use std::path::Path;
use hreinsa::sanitize;
use read_fonts::FontRef;

#[test]
fn test_ots_corpus_good_fonts() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let good_dir = workspace_root.join("target/ots/tests/fonts/good");
    if !good_dir.exists() {
        eprintln!("target/ots not found at {}, skipping OTS corpus test", good_dir.display());
        return;
    }

    let mut passed = 0;
    let mut total = 0;
    let mut failures = Vec::new();

    for entry in fs::read_dir(good_dir).expect("failed to read good fonts dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        // WOFF is out of scope for initial milestone
        if ext != "ttf" && ext != "otf" {
            continue;
        }

        total += 1;
        let data = fs::read(&path).expect("failed to read font file");
        match sanitize(&data) {
            Ok(sanitized) => {
                let parse_res = if sanitized.starts_with(b"ttcf") {
                    FontRef::from_index(&sanitized, 0)
                } else {
                    FontRef::new(&sanitized)
                };
                if let Err(e) = parse_res {
                    failures.push((path.display().to_string(), format!("FontRef failed: {e:?}")));
                } else {
                    passed += 1;
                }
            }
            Err(e) => {
                failures.push((path.display().to_string(), format!("Sanitize failed: {e}")));
            }
        }
    }

    println!("OTS good corpus: {passed}/{total} fonts passed");
    if !failures.is_empty() {
        for (f, err) in &failures {
            println!("Failure in {f}: {err}");
        }
    }
    // We expect the vast majority of valid fonts to pass
    assert!(
        passed >= total * 9 / 10,
        "Expected at least 90% of good fonts to pass, got {passed}/{total}"
    );
}

#[test]
fn test_ots_corpus_bad_fonts() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let bad_dir = workspace_root.join("target/ots/tests/fonts/bad");
    if !bad_dir.exists() {
        eprintln!("target/ots not found at {}, skipping OTS bad corpus test", bad_dir.display());
        return;
    }

    let mut rejected = 0;
    let mut total = 0;

    for entry in fs::read_dir(bad_dir).expect("failed to read bad fonts dir") {
        let entry = entry.unwrap();
        let path = entry.path();
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "ttf" && ext != "otf" && ext != "ttc" {
            continue;
        }

        total += 1;
        let data = fs::read(&path).expect("failed to read font file");
        if sanitize(&data).is_err() {
            rejected += 1;
        }
    }

    println!("OTS bad corpus: {rejected}/{total} rejected");
    // Many bad fonts specifically target tables we sanitize (head, maxp, loca, glyf, cmap, os2, etc.)
    assert!(
        rejected > 0,
        "Expected bad fonts to be rejected, got {rejected}/{total}"
    );
}
