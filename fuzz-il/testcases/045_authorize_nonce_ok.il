# source: test_process_authorize_ix_ok
LoadAddress authority = account:0
AuthorizeNonceAccount | authority ;
  (account:0, true, false),
  (account:1, false, true)
