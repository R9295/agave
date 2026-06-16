# source: test_transfer_lamports
LoadU64 lamports = 50
Transfer | lamports ;
  (account:0, true, true),
  (account:1, true, false)
