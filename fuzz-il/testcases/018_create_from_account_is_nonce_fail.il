# source: test_create_from_account_is_nonce_fail
# account:1 is an initialized nonce account
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 42
LoadU64 space = 0
LoadAddress owner = harness
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
