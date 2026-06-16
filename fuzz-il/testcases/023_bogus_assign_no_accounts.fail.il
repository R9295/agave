# source: test_process_bogus_instruction
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAddress owner = program
Assign | owner ;
