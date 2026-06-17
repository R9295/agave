# source: test_process_withdraw_ix_only_nonce_acc_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | hex:010000000100000066757a7a2d696c2d6163636f756e74000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:1, true, false)
