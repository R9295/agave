# source: system_instruction.rs initialize_inx_initialized_account_fail
# nonce account is already initialized
LoadAccountState account:0 NonceInitialized account:0 0
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:0
InitializeNonceAccount | authority ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
