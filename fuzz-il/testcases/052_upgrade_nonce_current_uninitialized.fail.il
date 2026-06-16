# source: test_nonce_account_upgrade
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 NonceUninitialized
UpgradeNonceAccount | ;
  (account:1, true, false)
