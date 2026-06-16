# source: test_nonce_advance_with_empty_recent_blockhashes_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitialized account:2 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashesEmpty
LoadAccountState sysvar:rent SysvarRent
AdvanceNonceAccount | ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:2, false, true)
