# source: test_process_bogus_instruction
# harness should provide only account:0
LoadU64 lamports = 0
Transfer | lamports ; (account:0, true, true)
