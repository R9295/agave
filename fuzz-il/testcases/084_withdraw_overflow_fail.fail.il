# source: system_instruction.rs withdraw_inx_overflow
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitialized account:3 100
LoadAccountState account:2 ForeignData 18446744073709551560 zeros:0 harness
LoadAccountState account:3 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 100
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
