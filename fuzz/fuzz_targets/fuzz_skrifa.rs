#![no_main]
use libfuzzer_sys::fuzz_target;
use skrifa::FontRef;

fn do_skrifa_things(data: &[u8]) -> Result<(), String> {
    let _font = FontRef::new(data).map_err(|e| format!("{e}"))?;

    // TODO: put skrifa through it's paces

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = do_skrifa_things(data);
});
