# source: system_instruction.rs nonce_inx_uninitialized_account_fail
# nonce account data is valid but state is uninitialized
LoadAccountState account:0 NonceUninitialized
LoadAccountState account:1 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false),
  (account:1, false, true)
