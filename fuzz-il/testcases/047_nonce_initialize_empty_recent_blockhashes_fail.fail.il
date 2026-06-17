# source: test_nonce_initialize_with_empty_recent_blockhashes_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | hex:0100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashesEmpty
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
