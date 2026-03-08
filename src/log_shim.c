#include <stdio.h>
#include <stdarg.h>
#include "esp_log.h"

/* Declared in log_relay.rs as #[no_mangle] extern "C" */
extern void rust_log_try_send(const char *msg, size_t len);
extern int  rust_log_is_reentering(void);

static vprintf_like_t s_original_vprintf = NULL;

static int mqtt_log_vprintf(const char *fmt, va_list args) {
    /* Always call original first — preserves UART output */
    int ret = s_original_vprintf(fmt, args);

    /* Re-entrancy guard: skip MQTT path if called from the log relay thread */
    if (rust_log_is_reentering()) {
        return ret;
    }

    /* Format to stack buffer — 256 bytes covers typical log lines; truncates silently */
    char buf[256];
    va_list args2;
    va_copy(args2, args);  /* MUST copy: s_original_vprintf already consumed args */
    int n = vsnprintf(buf, sizeof(buf), fmt, args2);
    va_end(args2);

    if (n > 0) {
        size_t len = (n < (int)sizeof(buf)) ? (size_t)n : sizeof(buf) - 1;
        rust_log_try_send(buf, len);
    }
    return ret;
}

void install_mqtt_log_hook(void) {
    s_original_vprintf = esp_log_set_vprintf(mqtt_log_vprintf);
}
