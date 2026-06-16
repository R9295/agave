# source: test_transfer_with_seed
# accounts: derived sender, account:1=base signer, account:2=recipient
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState 0x0ac209f802f8951bc8c4bb3291c5f3909199df55b1b03562fa6d145f2e14d089 SystemFunded 100
LoadAccountState account:1 ForeignData 100 zeros:0 harness
LoadAccountState account:2 ForeignData 1 zeros:0 harness
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 50, seed, owner ;
  (0x0ac209f802f8951bc8c4bb3291c5f3909199df55b1b03562fa6d145f2e14d089, true, false),
  (account:1, false, true),
  (account:2, true, false)
