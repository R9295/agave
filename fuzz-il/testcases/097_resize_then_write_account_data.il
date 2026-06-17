# AccountResize can expose data that WriteAccountData then fills.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | harness | 1000000000
LoadU64 new_len = 8
AccountResize | new_len ;
  (account:1, true, false)
LoadU64 offset = 4
LoadU64 len = 4
LoadU8 value = 17
WriteAccountData | offset, len, value ;
  (account:1, true, false)
