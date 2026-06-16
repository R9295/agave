# source: system_instruction.rs initialize_inx_initialized_account_fail
# nonce account is already initialized
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitialized account:1 0
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
