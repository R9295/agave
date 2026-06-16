# source: test_transfer_with_seed
# accounts: 0=from derived, 1=base signer, 2=recipient
LoadAccountState account:0 SystemFunded 100
LoadAccountState account:1 ForeignData 100 zeros:0 harness
LoadAccountState account:2 ForeignData 1 zeros:0 harness
LoadString seed = "42"
LoadAddress owner = system
TransferWithSeed | 50, seed, owner ;
  (account:0, true, false),
  (account:1, false, true),
  (account:2, true, false)
