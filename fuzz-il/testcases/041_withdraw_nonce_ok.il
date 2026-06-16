# source: test_process_withdraw_ix_ok
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
