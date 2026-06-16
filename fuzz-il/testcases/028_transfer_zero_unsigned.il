# source: test_transfer_lamports
# harness should clear account:0 signer bit
LoadAccountState account:0 SystemFunded 100
LoadAccountState account:1 ForeignData 1 zeros:0 harness
LoadU64 lamports = 0
Transfer | lamports ;
  (account:0, true, false),
  (account:1, true, false)
