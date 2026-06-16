# source: test_create_data_populated
# account:1 starts with populated data
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
