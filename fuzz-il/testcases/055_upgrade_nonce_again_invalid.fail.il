# source: test_nonce_account_upgrade
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | hex:010000000100000066757a7a2d696c2d6163636f756e74000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000 | system | 1447680
UpgradeNonceAccount | ;
  (account:1, true, false)
