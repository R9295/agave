# source: test_transfer_lamports
LoadAccountState account:0 SystemFunded 100
LoadAccountState account:1 ForeignData 1 zeros:0 harness
LoadU64 lamports = 50
Transfer | lamports ;
  (account:0, true, true),
  (account:1, true, false)
