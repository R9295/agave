# source: test_create_unsigned
# harness should clear both signer bits
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ; (account:0, true, false), (account:1, true, false)
