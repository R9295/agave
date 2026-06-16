# source: test_nonce_account_upgrade
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceInitialized account:1 0
UpgradeNonceAccount | ;
  (account:1, false, false)
