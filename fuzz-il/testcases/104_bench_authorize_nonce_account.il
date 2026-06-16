# source: programs/system/benches/system.rs bench_authorize_nonce_account
LoadAddress authority = account:0
AuthorizeNonceAccount | authority ; (account:0, true, false), (account:1, false, true)
