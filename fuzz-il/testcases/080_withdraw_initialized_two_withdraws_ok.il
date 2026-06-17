# source: system_instruction.rs withdraw_inx_initialized_acc_two_withdraws_ok
# nonce account starts initialized, partial withdraw preserves state, final withdraw deinitializes
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | hex:010000000100000066757a7a2d696c2d6163636f756e74000000000000000000030000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 2447722
LoadAccountState account:2 | zeros:0 | system | 0
LoadAccountState account:3 | zeros:0 | system | 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 partial = 42
WithdrawNonceAccount | partial ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
LoadU64 remaining = 1000000
WithdrawNonceAccount | remaining ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
