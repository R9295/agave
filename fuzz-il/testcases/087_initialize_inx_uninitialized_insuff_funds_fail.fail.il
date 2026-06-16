# source: system_instruction.rs initialize_inx_uninitialized_acc_insuff_funds_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceInitializedLowRent account:1
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = account:1
InitializeNonceAccount | authority ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false)
