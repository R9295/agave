# source: test_assign
# assigning to current system owner needs no signature
LoadAddress owner = system
Assign | owner ;
  (account:0, true, false)
