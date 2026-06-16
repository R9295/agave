# source: test_assign
# harness should clear account:0 signer bit
LoadAccountState account:0 SystemFunded 100
LoadAddress owner = 0x0909090909090909090909090909090909090909090909090909090909090909
Assign | owner ;
  (account:0, true, false)
