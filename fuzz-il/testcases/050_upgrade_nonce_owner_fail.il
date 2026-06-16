# source: test_nonce_account_upgrade_check_owner
# nonce account owner is not the system program
LoadAccountState account:0 ForeignData 1000000000 zeros:80 harness
UpgradeNonceAccount | ;
  (account:0, true, false)
