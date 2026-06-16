# source: test_process_initialize_ix_no_keyed_accs_fail
LoadAccountState sysvar:clock SysvarClock
LoadAccountState sysvar:rent SysvarRent
LoadAddress authority = system
InitializeNonceAccount | authority ;
