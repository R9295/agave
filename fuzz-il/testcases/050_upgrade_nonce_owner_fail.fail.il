# source: test_nonce_account_upgrade_check_owner
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:80 | harness | 1000000000
UpgradeNonceAccount | ;
  (account:1, true, false)
