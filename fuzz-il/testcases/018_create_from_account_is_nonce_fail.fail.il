# source: test_create_from_account_is_nonce_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | hex:010000000100000066757a7a2d696c2d6163636f756e74000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
LoadAccountState account:2 | zeros:0 | system | 0
LoadU64 lamports = 42
LoadU64 space = 0
LoadAddress owner = program
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
