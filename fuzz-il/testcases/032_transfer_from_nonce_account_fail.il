# source: test_transfer_lamports_from_nonce_account_fail
# account:0 is an initialized nonce account
LoadAccountState account:0 NonceInitialized account:0 0
LoadAccountState account:1 ForeignData 1 zeros:0 harness
LoadU64 lamports = 50
Transfer | lamports ;
  (account:0, true, true),
  (account:1, true, false)
