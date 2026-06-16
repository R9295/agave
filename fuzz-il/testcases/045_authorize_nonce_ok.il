# source: test_process_authorize_ix_ok
LoadAccountState account:0 NonceInitialized account:1 0
LoadAccountState account:1 SystemFunded 1000000000
LoadAddress authority = account:0
AuthorizeNonceAccount | authority ;
  (account:0, true, false),
  (account:1, false, true)
