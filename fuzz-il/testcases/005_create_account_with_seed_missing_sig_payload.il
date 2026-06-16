# source: test_create_account_with_seed_missing_sig
# harness should clear the created-account signature/writable bits
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ; (account:0, false, true), (account:1, false, false)
