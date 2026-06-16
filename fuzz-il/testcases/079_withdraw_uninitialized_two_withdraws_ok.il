# source: system_instruction.rs withdraw_inx_uninitialized_acc_two_withdraws_ok
# nonce account is uninitialized and remains uninitialized after each withdraw
LoadAccountState account:0 NonceUninitialized
LoadAccountState account:1 SystemEmpty
LoadAccountState account:2 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 first = 500021
WithdrawNonceAccount | first ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
LoadU64 second = 500021
WithdrawNonceAccount | second ;
  (account:0, true, false),
  (account:1, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:2, false, true)
