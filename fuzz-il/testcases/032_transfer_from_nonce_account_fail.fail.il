# source: test_transfer_lamports_from_nonce_account_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
LoadAccountState account:2 SystemEmpty
LoadU64 lamports = 1
Transfer | lamports ;
  (account:1, true, true),
  (account:2, true, false)
