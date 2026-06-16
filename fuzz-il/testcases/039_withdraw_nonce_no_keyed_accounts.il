# source: test_process_withdraw_ix_no_keyed_accs_fail
# harness should provide zero keyed accounts
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 42
WithdrawNonceAccount | lamports ;
