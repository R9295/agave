# source: test_process_authorize_ix_ok
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:2 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAddress authority = account:1
AuthorizeNonceAccount | authority ;
  (account:1, true, false),
  (account:2, false, true)
