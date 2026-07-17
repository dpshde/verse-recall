#include "recall.h"

#include <stddef.h>
#include <string.h>

int main(void) {
    if (recall_abi_version() == 0) {
        return 1;
    }

    recall_err error = RECALL_OK;
    recall_engine *engine = recall_open_v2(NULL, NULL, 0, &error);
    if (engine != NULL || error != RECALL_ERR_NULL) {
        return 2;
    }

    char *message = recall_last_error_message();
    if (message == NULL || strstr(message, "at least one pack") == NULL) {
        recall_free_str(message);
        return 3;
    }
    recall_free_str(message);

    recall_clear_last_error();
    message = recall_last_error_message();
    if (message != NULL) {
        recall_free_str(message);
        return 4;
    }

    recall_close(NULL);
    recall_free_str(NULL);
    return 0;
}
