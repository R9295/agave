# source: test_transfer_with_seed
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState 0x0ac209f802f8951bc8c4bb3291c5f3909199df55b1b03562fa6d145f2e14d089 | zeros:0 | system | 100
LoadAccountState account:1 | zeros:0 | harness | 100
LoadAccountState account:2 | zeros:0 | harness | 1
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 101, seed, owner ;
  (0x0ac209f802f8951bc8c4bb3291c5f3909199df55b1b03562fa6d145f2e14d089, true, false),
  (account:1, false, true),
  (account:2, true, false)
