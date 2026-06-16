# source: system_instruction.rs default_is_uninitialized
# harness provides account:0 with default/uninitialized nonce state
UpgradeNonceAccount | ;
  (account:0, true, false)
