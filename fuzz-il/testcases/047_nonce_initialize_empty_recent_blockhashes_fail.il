# source: test_nonce_initialize_with_empty_recent_blockhashes_fail
# recent-blockhashes sysvar account is present but empty
LoadAddress authority = account:0
InitializeNonceAccount | authority ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
