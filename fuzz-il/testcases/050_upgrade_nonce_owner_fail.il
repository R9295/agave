# source: test_nonce_account_upgrade_check_owner
# nonce account owner is not the system program
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 ForeignData 1000000000 zeros:80 harness
UpgradeNonceAccount | ;
  (account:1, true, false)
