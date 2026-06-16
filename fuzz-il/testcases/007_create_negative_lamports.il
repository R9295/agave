# source: test_create_negative_lamports
# harness funder balance: 100 lamports; instruction requests 150
LoadU64 lamports = 150
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
