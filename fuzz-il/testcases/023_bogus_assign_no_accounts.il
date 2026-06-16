# source: test_process_bogus_instruction
# harness should provide zero keyed accounts
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAddress owner = harness
Assign | owner ;
