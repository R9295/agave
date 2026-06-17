# AddAccountLamports and SubAccountLamports can roundtrip to the original balance.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | harness | 1000000000
LoadU64 delta = 7
AddAccountLamports | delta ;
  (account:1, true, false)
SubAccountLamports | delta ;
  (account:1, true, false)
