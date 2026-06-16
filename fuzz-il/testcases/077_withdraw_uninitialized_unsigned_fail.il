# source: system_instruction.rs withdraw_inx_unintialized_acc_unsigned_fail
# nonce account is uninitialized; harness clears account:3 signer
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemEmpty
LoadAccountState account:3 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 lamports = 1000042
WithdrawNonceAccount | lamports ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, false)
