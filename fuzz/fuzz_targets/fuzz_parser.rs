#![no_main]

use libfuzzer_sys::fuzz_target;
use ghl_syntax::parser::parse;
use ghl_types::check;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(prog) = parse(s) {
            let _ = check(&prog, "fuzz.gh");
        }
    }
});
