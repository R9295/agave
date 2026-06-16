# source: test_assign
LoadAccountState account:0 SystemFunded 100
LoadAddress owner = Sysvar1111111111111111111111111111111111111
Assign | owner ;
  (account:0, true, true)
