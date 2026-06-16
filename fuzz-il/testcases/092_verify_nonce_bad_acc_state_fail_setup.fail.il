# source: system_instruction.rs verify_nonce_bad_acc_state_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:2, false, true)
