/*
 * Plain C consumer for verse-recall.
 *
 * Build (from repo root after `cargo build -p recall-ffi`):
 *
 *   cc -std=c11 -Wall -Wextra -I crates/recall-ffi/include \
 *     examples/c/main.c \
 *     -L target/debug -lrecall_ffi \
 *     -o /tmp/recall-c-example \
 *     -lm -ldl -lpthread
 *
 *   /tmp/recall-c-example path/to/pack.pack "lost sheep"
 *
 * Without args, prints ABI version and exits 0 (link smoke test).
 */

#include "recall.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned char *read_file(const char *path, size_t *out_len) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        return NULL;
    }
    if (fseek(f, 0, SEEK_END) != 0) {
        fclose(f);
        return NULL;
    }
    long sz = ftell(f);
    if (sz < 0) {
        fclose(f);
        return NULL;
    }
    rewind(f);
    unsigned char *buf = (unsigned char *)malloc((size_t)sz);
    if (!buf) {
        fclose(f);
        return NULL;
    }
    size_t n = fread(buf, 1, (size_t)sz, f);
    fclose(f);
    if (n != (size_t)sz) {
        free(buf);
        return NULL;
    }
    *out_len = n;
    return buf;
}

int main(int argc, char **argv) {
    printf("recall_abi_version = %u\n", (unsigned)recall_abi_version());

    if (argc < 3) {
        printf("usage: %s <pack.pack> <query>\n", argv[0]);
        printf("(link smoke ok — pass pack + query to run a search)\n");
        return 0;
    }

    size_t len = 0;
    unsigned char *pack = read_file(argv[1], &len);
    if (!pack) {
        fprintf(stderr, "failed to read pack: %s\n", argv[1]);
        return 1;
    }

    const uint8_t *packs[1] = {pack};
    size_t lens[1] = {len};
    recall_err err = RECALL_OK;
    recall_engine *eng = recall_open(packs, lens, 1, &err);
    free(pack);

    if (!eng) {
        fprintf(stderr, "recall_open failed: %d\n", (int)err);
        return 1;
    }

    recall_opts opts;
    memset(&opts, 0, sizeof(opts));
    opts.top_k = 5;

    char *json = recall_search_json(eng, argv[2], &opts);
    if (!json) {
        fprintf(stderr, "search failed\n");
        recall_close(eng);
        return 1;
    }

    puts(json);
    recall_free_str(json);
    recall_close(eng);
    return 0;
}
