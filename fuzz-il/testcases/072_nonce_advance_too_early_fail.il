# source: system_instruction.rs nonce_inx_too_early_fail
# nonce is initialized and current blockhash has not expired
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
