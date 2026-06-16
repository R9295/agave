# source: system_instruction.rs initialize_inx_uninitialized_acc_insuff_funds_fail
# nonce account is uninitialized but below rent-exempt balance
LoadAccountState account:0 NonceUninitialized
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:0
InitializeNonceAccount | authority ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
