# WriteAccountData uses memset to fill existing account data.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:8 | harness | 1000000000
LoadU64 offset = 2
LoadU64 len = 3
LoadU8 value = 255
WriteAccountData | offset, len, value ;
  (account:1, true, false)
