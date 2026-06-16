# source: test_create_unsigned
# harness should clear account:1 signer bit
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, false)
