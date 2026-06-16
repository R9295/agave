# source: system_instruction.rs nonce_inx_too_early_fail
# nonce is initialized and current blockhash has not expired
LoadAccountState account:0 NonceInitialized account:1 0
LoadAccountState account:1 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
