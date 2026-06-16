# source: test_transfer_lamports_from_nonce_account_fail
# account:1 is an initialized nonce account
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
LoadAccountState account:2 ForeignData 1 zeros:0 harness
LoadU64 lamports = 50
Transfer | lamports ;
  (account:1, true, true),
  (account:2, true, false)
