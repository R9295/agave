# AccountResize can grow a harness-owned writable account by MAX_ACCOUNT_GROW_LIMIT.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | harness | 1000000000
LoadU64 new_len = 10240
AccountResize | new_len ;
  (account:1, true, false)
