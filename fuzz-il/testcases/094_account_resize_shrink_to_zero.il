# AccountResize can shrink a harness-owned writable account to zero bytes.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | hex:deadbeef | harness | 1000000000
LoadU64 new_len = 0
AccountResize | new_len ;
  (account:1, true, false)
