# source: system_instruction.rs nonce_inx_uninitialized_account_fail
# nonce account data is valid but state is uninitialized
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
