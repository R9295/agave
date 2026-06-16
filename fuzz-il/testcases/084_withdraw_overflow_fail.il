# source: system_instruction.rs withdraw_inx_overflow
# requested lamports is u64::MAX - 54
LoadU64 lamports = 18446744073709551561
WithdrawNonceAccount | lamports ; (account:0, true, false), (account:1, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false), (account:2, false, true)
