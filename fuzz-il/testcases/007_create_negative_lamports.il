# source: test_create_negative_lamports
# harness funder balance: 100 lamports; instruction requests 150
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 ForeignData 100 zeros:0 harness
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 150
LoadU64 space = 2
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
