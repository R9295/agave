# source: test_nonce_advance_with_empty_recent_blockhashes_fail
# recent-blockhashes sysvar account is present but empty
LoadAccountState account:0 NonceInitialized account:1 0
LoadAccountState account:1 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashesEmpty
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
