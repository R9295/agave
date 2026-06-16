# source: system_instruction.rs nonce_inx_too_early_fail
# nonce is initialized and current blockhash has not expired
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:2 0
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:2, false, true)
