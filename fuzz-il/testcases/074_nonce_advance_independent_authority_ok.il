# source: system_instruction.rs nonce_inx_independent_nonce_authority_ok
# nonce account authority is account:1, and account:1 signs
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
