# source: system_instruction.rs verify_nonce_ok
# entrypoint-equivalent setup that creates the initialized nonce state being verified
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 | hex:0100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
