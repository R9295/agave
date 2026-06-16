# source: test_process_withdraw_ix_only_nonce_acc_fail
# harness should provide only account:1
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:1, true, false)
