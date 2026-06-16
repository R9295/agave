# source: system_instruction.rs nonce_inx_initialized_account_not_signer_fail
# nonce is initialized with account:0 authority; harness clears authority signer
LoadAccountState account:0 NonceInitialized account:0 0
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
AdvanceNonceAccount | ;
  (account:0, true, false),
  (sysvar:recent_blockhashes, false, false)
