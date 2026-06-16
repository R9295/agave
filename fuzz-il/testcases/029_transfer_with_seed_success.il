# source: test_transfer_with_seed
# accounts: 0=from derived, 1=base signer, 2=recipient
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 50, seed, owner ; (account:0, true, false), (account:1, false, true), (account:2, true, false)
