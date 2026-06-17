# source: system_instruction.rs withdraw_inx_initialized_acc_nonce_too_early_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | hex:010000000100000066757a7a2d696c2d6163636f756e7400000000000000000003000000000000007a3bd5cdf290098e5b4c8f9957f9899210b8bbe9064f72b326cc212eab476f8a0000000000000000 | system | 1447680
LoadAccountState account:2 | zeros:0 | system | 0
LoadAccountState account:3 | zeros:0 | system | 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1447680
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
