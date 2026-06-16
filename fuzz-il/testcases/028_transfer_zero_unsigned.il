# source: test_transfer_lamports
# harness should clear account:0 signer bit
LoadU64 lamports = 0
Transfer | lamports ; (account:0, true, false), (account:1, true, false)
