# SetAccountOwner can write the existing owner value.
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAccountState account:1 | zeros:0 | harness | 1000000000
LoadAddress owner = harness
SetAccountOwner | owner ;
  (account:1, true, false)
