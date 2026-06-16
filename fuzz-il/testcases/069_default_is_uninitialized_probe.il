# source: system_instruction.rs default_is_uninitialized
# harness explicitly provides account:0 with uninitialized nonce state
LoadAccountState account:0 NonceUninitialized
UpgradeNonceAccount | ;
  (account:0, true, false)
