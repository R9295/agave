# source: test_nonce_advance_with_empty_recent_blockhashes_fail
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
