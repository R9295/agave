# source: test_create_from_account_is_nonce_fail
# account:0 is an initialized nonce account
LoadU64 lamports = 42
LoadU64 space = 0
LoadAddress owner = harness
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
