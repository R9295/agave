# source: test_nonce_account_upgrade
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | hex:0100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
UpgradeNonceAccount | ;
  (account:1, true, false)
