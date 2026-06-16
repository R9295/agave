# source: system_instruction.rs verify_nonce_ok
# entrypoint-equivalent setup that creates the initialized nonce state being verified
LoadAddress authority = account:0
InitializeNonceAccount | authority ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
