# source: system_instruction.rs withdraw_inx_initialized_acc_insuff_rent_fail
# initialized nonce account cannot drop below rent reserve
LoadAccountState account:0 NonceInitialized account:2 42
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 43
WithdrawNonceAccount | lamports ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
