# source: test_process_authorize_ix_ok
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false), (sysvar:recent_blockhashes, false, false), (sysvar:rent, false, false)
