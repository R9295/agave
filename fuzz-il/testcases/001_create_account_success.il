# source: test_create_account
# accounts: 0=funder signer+writable with 100 lamports, 1=new signer+writable empty
LoadU64 lamports = 50
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:0, true, true),
  (account:1, true, true)
