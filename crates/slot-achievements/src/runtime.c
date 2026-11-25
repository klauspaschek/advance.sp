/* Small C ABI around the unmodified rcheevos runtime. Only the RA worker calls it. */
#include "rc_runtime.h"
#include <stdint.h>
#include <stddef.h>

typedef struct {
    const uint8_t *ram;
    const size_t *starts;
    const size_t *valid;
    uint32_t *earned;
    size_t capacity, count;
} slot_frame;

/* The runtime's event/validation callbacks have no userdata argument. */
static _Thread_local slot_frame *current;

/* Three regions at ascending RetroAchievements addresses; starts[0] is always 0. */
static int valid_address(uint32_t address) {
    unsigned region = 2;
    while (region > 0 && address < current->starts[region]) --region;
    return address - current->starts[region] < current->valid[region];
}

/* Like rc_client, a read outside the snapshot is 0. Pointer-based sets (Pokémon
 * Emerald) follow pointers that are briefly or permanently out of range for some of
 * their conditions on every frame; discarding the whole frame then would reset every
 * achievement each frame, so none could ever trigger. Directly addressed invalid
 * memory is still disabled by rc_runtime_validate_addresses. */
static uint32_t peek(uint32_t address, uint32_t bytes, void *unused) {
    uint32_t value = 0, i;
    (void)unused;
    for (i = 0; i < bytes; ++i) {
        if (address > UINT32_MAX - i || !valid_address(address + i))
            return 0;
        value |= ((uint32_t)current->ram[address + i]) << (8 * i);
    }
    return value;
}

static void event(const rc_runtime_event_t *ev) {
    if (ev->type == RC_RUNTIME_EVENT_ACHIEVEMENT_TRIGGERED && current->count < current->capacity)
        current->earned[current->count++] = ev->id;
}

size_t slot_ra_frame(rc_runtime_t *runtime, const uint8_t *ram, const size_t *starts,
                     const size_t *valid, uint32_t *earned, size_t capacity, char *presence,
                     size_t presence_size) {
    slot_frame frame = { ram, starts, valid, earned, capacity, 0 };
    current = &frame;
    rc_runtime_validate_addresses(runtime, event, valid_address);
    rc_runtime_do_frame(runtime, event, peek, NULL, NULL);
    rc_runtime_get_richpresence(runtime, presence, presence_size, peek, NULL, NULL);
    presence[presence_size - 1] = 0;
    current = NULL;
    return frame.count;
}
