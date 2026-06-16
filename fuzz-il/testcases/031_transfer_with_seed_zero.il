# source: test_transfer_with_seed
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 SystemFunded 100
LoadAccountState account:2 ForeignData 100 zeros:0 harness
LoadAccountState account:3 ForeignData 1 zeros:0 harness
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 0, seed, owner ;
  (account:1, true, false),
  (account:2, false, true),
  (account:3, true, false)
