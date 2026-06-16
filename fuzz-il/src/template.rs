/// Solana SDK header bundle: typedefs + syscall prototypes + a generated
/// `entrypoint` that walks the SBPF input region into `SolParameters`. The
/// `assemble` step splices the per-program `user_body` into this just before
/// `entrypoint`, and the body call just before `entrypoint`'s `return 0;`.
pub const TEMPLATE: &str = "
/* Types */
typedef signed char int8_t;
typedef unsigned char uint8_t;
typedef signed short int16_t;
typedef unsigned short uint16_t;
typedef signed int int32_t;
typedef unsigned int uint32_t;
typedef signed long int int64_t;
typedef unsigned long int uint64_t;
typedef int64_t ssize_t;
typedef uint64_t size_t;
typedef struct {
  const uint8_t *addr;
  uint64_t len;
} SolBytes;
typedef struct {
  uint8_t x[32];
} SolPubkey;

/* Syscalls */
void sol_memcpy_(void *, const void *, uint64_t);
void sol_memmove_(void *, const void *, uint64_t);
void sol_memset_(void *, uint8_t, uint64_t);
void sol_memcmp_(const void *, const void *, uint64_t, int32_t *);
void sol_set_return_data(const uint8_t *, uint64_t);
uint64_t sol_get_return_data(uint8_t *, uint64_t, SolPubkey *);
uint64_t sol_remaining_compute_units(void);
void sol_get_clock_sysvar(uint8_t *);
void sol_get_epoch_schedule_sysvar(uint8_t *);
void sol_get_rent_sysvar(uint8_t *);
void sol_get_last_restart_slot(uint8_t *);
void sol_get_epoch_rewards_sysvar(uint8_t *);
uint64_t sol_get_sysvar(const uint8_t *, uint8_t *, uint64_t, uint64_t);
uint64_t sol_get_stack_height(void);
uint64_t sol_get_epoch_stake(const uint8_t *);
uint64_t sol_get_processed_sibling_instruction(uint64_t, void *, uint8_t *, uint8_t *, uint8_t *);

/* Account Types */
typedef struct {
  const uint8_t *addr;
  uint64_t len;
} SolSignerSeed;

typedef struct {
  const SolSignerSeed *addr;
  uint64_t len;
} SolSignerSeeds;

uint64_t sol_create_program_address(const SolSignerSeed *, int, const SolPubkey *, SolPubkey *);
uint64_t sol_try_find_program_address(const SolSignerSeed *, int, const SolPubkey *, SolPubkey *, uint8_t *);

typedef struct {
  SolPubkey *key;
  uint64_t *lamports;
  uint64_t data_len;
  uint8_t *data;
  SolPubkey *owner;
  uint64_t rent_epoch;
  _Bool is_signer;
  _Bool is_writable;
  _Bool executable;
} SolAccountInfo;

typedef struct {
  SolAccountInfo* ka;
  uint64_t ka_num;
  const uint8_t *data;
  uint64_t data_len;
  const SolPubkey *program_id;
} SolParameters;

uint64_t entrypoint(const uint8_t *input);

typedef struct {
  SolPubkey *pubkey;
  _Bool is_writable;
  _Bool is_signer;
} SolAccountMeta;

typedef struct {
  SolPubkey *program_id;
  SolAccountMeta *accounts;
  uint64_t account_len;
  uint8_t *data;
  uint64_t data_len;
} SolInstruction;

uint64_t sol_invoke_signed_c(
  const SolInstruction *,
  const SolAccountInfo *,
  int,
  const SolSignerSeeds *,
  int
);

/* Pubkey constants used by generated code. Defined once in the template */
static const SolPubkey SYSTEM_PROGRAM_ID = { .x = {0} };
static const SolPubkey HARNESS_PROGRAM_ID = { .x = {
  0xa1, 0xb2, 0xc3, 0xd4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef
} };
static const SolPubkey SYSVAR_CLOCK_ID = { .x = {
  6, 167, 213, 23, 24, 199, 116, 201, 40, 86, 99, 152, 105, 29, 94, 182,
  139, 94, 184, 163, 155, 75, 109, 92, 115, 85, 91, 33, 0, 0, 0, 0
} };
static const SolPubkey SYSVAR_RENT_ID = { .x = {
  6, 167, 213, 23, 25, 47, 10, 175, 198, 242, 101, 227, 251, 119, 204, 122,
  218, 130, 197, 41, 208, 190, 59, 19, 110, 45, 0, 85, 32, 0, 0, 0
} };
static const SolPubkey SYSVAR_RECENT_BLOCKHASHES_ID = { .x = {
  6, 167, 213, 23, 25, 44, 86, 142, 224, 138, 132, 95, 115, 210, 151, 136,
  207, 3, 92, 49, 69, 178, 26, 179, 68, 216, 6, 46, 169, 64, 0, 0
} };
static const SolPubkey SYSVAR_EPOCH_SCHEDULE_ID = { .x = {
  6, 167, 213, 23, 24, 220, 63, 238, 2, 211, 64, 70, 47, 247, 80, 215,
  227, 84, 11, 26, 215, 23, 158, 192, 12, 100, 110, 175, 64, 0, 0, 0
} };
static const SolPubkey SYSVAR_EPOCH_REWARDS_ID = { .x = {
  6, 167, 213, 23, 24, 219, 192, 4, 178, 82, 211, 122, 242, 80, 71, 138,
  167, 246, 234, 92, 144, 27, 245, 23, 31, 173, 4, 25, 16, 0, 0, 0
} };
static const SolPubkey SYSVAR_LAST_RESTART_SLOT_ID = { .x = {
  6, 167, 213, 23, 24, 138, 113, 244, 87, 27, 95, 209, 168, 250, 245, 196,
  217, 219, 152, 247, 19, 6, 33, 22, 86, 68, 100, 18, 88, 0, 0, 0
} };

/* Helper Functions */
static uint64_t align_up_8(uint64_t x) {
    return (x + 7) & ~((uint64_t)7);
}

static _Bool custom_accounts_deserialize(
    const uint8_t *input,
    SolParameters *params,
    uint64_t ka_capacity
) {
    if (input == 0 || params == 0) {
        return 0;
    }
    uint8_t *cur = (uint8_t *)input;
    params->ka_num = *(uint64_t *)cur;
    cur += sizeof(uint64_t);
    if (params->ka_num > ka_capacity) {
        return 0;
    }
    for (uint64_t i = 0; i < params->ka_num; i++) {
        uint8_t dup = *cur;
        cur += sizeof(uint8_t);
        if (dup != 0xFF) {
            params->ka[i] = params->ka[dup];
            cur += 7;
            continue;
        }
        SolAccountInfo *ai = &params->ka[i];
        ai->is_signer = (*cur++ != 0);
        ai->is_writable = (*cur++ != 0);
        ai->executable = (*cur++ != 0);
        cur += sizeof(uint32_t);
        ai->key = (SolPubkey *)cur;
        cur += sizeof(SolPubkey);
        ai->owner = (SolPubkey *)cur;
        cur += sizeof(SolPubkey);
        ai->lamports = (uint64_t *)cur;
        cur += sizeof(uint64_t);
        ai->data_len = *(uint64_t *)cur;
        cur += sizeof(uint64_t);
        ai->data = cur;
        cur += ai->data_len;
        cur += (10 * 1024);
        cur = (uint8_t *)align_up_8((uint64_t)cur);
        ai->rent_epoch = *(uint64_t *)cur;
        cur += sizeof(uint64_t);
    }
    params->data_len = *(uint64_t *)cur;
    cur += sizeof(uint64_t);
    params->data = cur;
    cur += params->data_len;
    params->program_id = (SolPubkey *)cur;
    return 1;
}
/* Entrypoint */
extern uint64_t entrypoint(const uint8_t *input) {
    SolAccountInfo accounts[16];
    SolParameters params = (SolParameters){.ka = accounts};
    if (!custom_accounts_deserialize(input, &params, (sizeof(accounts) / sizeof(accounts[0])))) {
        return ((uint64_t)(2) << 32);
    }
    return 0;
}";
