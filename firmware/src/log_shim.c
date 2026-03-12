#include <stdio.h>
#include <stdarg.h>
#include "esp_log.h"

/* Declared in log_relay.rs as #[no_mangle] extern "C" */
extern void rust_log_try_send(const char *msg, size_t len);
extern int  rust_log_is_reentering(void);

static vprintf_like_t s_original_vprintf = NULL;

static int mqtt_log_vprintf(const char *fmt, va_list args) {
    /* Re-entrancy guard: skip MQTT path if called from the log relay thread */
    if (rust_log_is_reentering()) {
        return s_original_vprintf(fmt, args);
    }

    /* Format to stack buffer BEFORE calling original — va_copy must happen before
     * s_original_vprintf consumes args. EspLogger passes "%s" + pre-formatted string;
     * after the original vprintf consumes the string pointer, args is exhausted and
     * a post-call va_copy produces garbage. Copy first, then both paths get valid args. */
    char buf[256];
    va_list args2;
    va_copy(args2, args);
    int n = vsnprintf(buf, sizeof(buf), fmt, args2);
    va_end(args2);

    /* Always call original — preserves UART output */
    int ret = s_original_vprintf(fmt, args);

    if (n > 0) {
        size_t len = (n < (int)sizeof(buf)) ? (size_t)n : sizeof(buf) - 1;
        rust_log_try_send(buf, len);
    }
    return ret;
}

void install_mqtt_log_hook(void) {
    s_original_vprintf = esp_log_set_vprintf(mqtt_log_vprintf);
}
