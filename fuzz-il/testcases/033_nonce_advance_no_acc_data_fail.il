# source: test_process_nonce_ix_no_acc_data_fail
# account:0 has no nonce account data
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
