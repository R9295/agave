# source: test_process_withdraw_ix_only_nonce_acc_fail
# harness should provide only account:0
LoadAccountState account:0 NonceInitialized account:0 0
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
  (account:0, true, false)
