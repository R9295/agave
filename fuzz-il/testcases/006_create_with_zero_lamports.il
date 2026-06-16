# source: test_create_with_zero_lamports
# funder may be non-system-owned because no lamports are debited
LoadU64 lamports = 0
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
