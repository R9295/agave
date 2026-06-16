# source: test_process_initialize_ix_only_nonce_acc_fail
# harness should provide only account:1
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceUninitialized
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false)
