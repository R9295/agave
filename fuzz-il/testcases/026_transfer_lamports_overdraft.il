# source: test_transfer_lamports
# account:0 has 100 lamports; request 101
LoadU64 lamports = 101
Transfer | lamports ;
  (account:0, true, true),
  (account:1, true, false)
