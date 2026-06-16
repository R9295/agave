# source: system_instruction.rs verify_nonce_bad_query_hash_fail
# initialize nonce, then harness verifies against a later/different blockhash
LoadAddress authority = account:0
InitializeNonceAccount | authority ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
