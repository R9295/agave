#![no_main]

use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    if agave_votor::scenarios::run(data) {
        Corpus::Keep
    } else {
        Corpus::Reject
    }
});
