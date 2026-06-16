# source: test_process_initialize_ix_only_nonce_acc_fail
# harness should provide only account:0
LoadAddress authority = account:0
InitializeNonceAccount | authority ; (account:0, true, false)
