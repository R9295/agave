# source: test_nonce_advance_with_empty_recent_blockhashes_fail
# recent-blockhashes sysvar account is present but empty
AdvanceNonceAccount | ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (account:1, false, true)
