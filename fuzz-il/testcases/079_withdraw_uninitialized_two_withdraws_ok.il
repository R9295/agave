# source: system_instruction.rs withdraw_inx_uninitialized_acc_two_withdraws_ok
# nonce account is uninitialized and remains uninitialized after each withdraw
LoadAccountState sysvar:clock SysvarClock
LoadAccountState account:1 NonceUninitialized
LoadAccountState account:2 SystemEmpty
LoadAccountState account:3 SystemFunded 1000000000
LoadAccountState sysvar:recent_blockhashes SysvarRecentBlockhashes
LoadAccountState sysvar:rent SysvarRent
LoadU64 first = 500021
WithdrawNonceAccount | first ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
LoadU64 second = 500021
WithdrawNonceAccount | second ;
  (account:1, true, false),
  (account:2, true, false),
  (sysvar:recent_blockhashes, false, false),
  (sysvar:rent, false, false),
  (account:3, false, true)
