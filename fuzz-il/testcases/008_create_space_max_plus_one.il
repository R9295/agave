# source: test_request_more_than_allowed_data_length
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 50
LoadU64 space = 10485761
LoadAddress owner = system
CreateAccount | lamports, space, owner ;
  (account:1, true, true),
  (account:2, true, true)
