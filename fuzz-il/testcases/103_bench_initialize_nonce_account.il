# source: programs/system/benches/system.rs bench_initialize_nonce_account
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
