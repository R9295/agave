# source: test_create_sysvar_invalid_id_with_feature
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = Sysvar1111111111111111111111111111111111111
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
