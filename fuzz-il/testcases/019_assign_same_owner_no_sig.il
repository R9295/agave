# source: test_assign
# assigning to current system owner needs no signature
LoadAccountState account:0 SystemFunded 100
LoadAddress owner = system
Assign | owner ;
  (account:0, true, false)
