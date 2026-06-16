# source: test_process_withdraw_ix_only_nonce_acc_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitialized account:1 0
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:1, true, false)
