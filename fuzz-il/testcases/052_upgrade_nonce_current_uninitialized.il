# source: test_nonce_account_upgrade
LoadAccountState account:0 NonceUninitialized
UpgradeNonceAccount | ;
  (account:0, true, false)
