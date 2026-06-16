# source: test_nonce_account_upgrade
# harness should make account:0 not writable
UpgradeNonceAccount | ;
  (account:0, false, false)
