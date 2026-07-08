fn main() {
    if let Err(error) = fuzz_il::run() {
        eprintln!("fuzz-il: {error}");
        std::process::exit(1);
    }
}
