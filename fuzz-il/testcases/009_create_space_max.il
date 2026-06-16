# source: test_request_more_than_allowed_data_length
LoadU64 lamports = 50
LoadU64 space = 10485760
LoadAddress owner = system
CreateAccount | lamports, space, owner ; (account:0, true, true), (account:1, true, true)
