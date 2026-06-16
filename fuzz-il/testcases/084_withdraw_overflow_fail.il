# source: system_instruction.rs withdraw_inx_overflow
# requested lamports is u64::MAX - 54
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitialized account:3 1000042
LoadAccountState account:2 SystemEmpty
LoadAccountState account:3 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 18446744073709551561
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
