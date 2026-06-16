# source: test_process_nonce_ix_only_nonce_acc_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
AdvanceNonceAccount | ;
  (account:1, true, false)
