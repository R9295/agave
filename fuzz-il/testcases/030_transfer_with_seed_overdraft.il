# source: test_transfer_with_seed
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 101, seed, owner ; (account:0, true, false), (account:1, false, true), (account:2, true, false)
