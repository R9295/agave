# source: system_instruction.rs verify_nonce_bad_query_hash_fail
# initialize nonce, then harness verifies against a later/different blockhash
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
