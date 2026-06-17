# AccountResize grow requests above MAX_ACCOUNT_GROW_LIMIT are silent no-ops.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | harness | 1000000000
LoadU64 new_len = 10241
AccountResize | new_len ;
  (account:1, true, false)
