# source: system_instruction.rs nonce_inx_initialized_account_not_signer_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:2 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:2, false, false)
