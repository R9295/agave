Command line used to find this crash:

/Users/aarnav/.local/share/afl.rs/rustc-1.97.0-nightly-c935696/afl.rs-0.18.2/afl/bin/afl-fuzz -c0 -Ssecondaryfuzzer3 -i./output/program_cache_production/corpus/ -plin -o./output/program_cache_production/afl -g1 -G1048576 -l1 -P600 /Users/aarnav/projects/anza/agave/program-runtime/fuzz/target/afl/debug/program_cache_production

If you can't reproduce a bug outside of afl-fuzz, be sure to set the same
memory limit. The limit used for this fuzzing session was 0 B.

Need a tool to minimize test cases before investigating the crashes or sending
them to a vendor? Check out the afl-tmin that comes with the fuzzer!

Found any cool bugs in open-source tools using afl-fuzz? If yes, please post
to https://github.com/AFLplusplus/AFLplusplus/issues/286 once the issues
 are fixed :)

