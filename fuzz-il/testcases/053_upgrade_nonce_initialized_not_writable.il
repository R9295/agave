# source: test_nonce_account_upgrade
# harness should make account:0 not writable
LoadAccountState account:0 NonceInitialized account:0 0
UpgradeNonceAccount | ;
  (account:0, false, false)
