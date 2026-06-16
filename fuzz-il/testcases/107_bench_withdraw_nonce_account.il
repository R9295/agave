# source: programs/system/benches/system.rs bench_withdraw_nonce_account
LoadU64 lamports = 1
WithdrawNonceAccount | lamports ; (account:0, true, false), (account:1, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false), (account:2, false, true)
