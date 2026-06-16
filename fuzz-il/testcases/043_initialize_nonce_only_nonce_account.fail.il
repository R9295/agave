# source: test_process_initialize_ix_only_nonce_acc_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false)
