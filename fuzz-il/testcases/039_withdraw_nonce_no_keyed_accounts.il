# source: test_process_withdraw_ix_no_keyed_accs_fail
# harness should provide zero keyed accounts
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
