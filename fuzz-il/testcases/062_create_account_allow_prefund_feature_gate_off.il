# source: test_create_account_allow_prefund
# feature set disables create_account_allow_prefund
LoadAccountState account:0 SystemFunded 100
LoadAccountState account:1 SystemFunded 100
LoadU64 lamports = 50
LoadU64 space = 0
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccountAllowPrefund | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
