# source: test_create_sysvar_invalid_id_with_feature
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = Sysvar1111111111111111111111111111111111111
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
