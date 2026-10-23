#include <stdio.h>
#include <stdlib.h>
#include "../frame-bar.h"

int main(int argc, char **argv) {
    if (argc != 4) return 1;
    int width = atoi(argv[1]), height = atoi(argv[2]);
    uint32_t id = strtoul(argv[3], NULL, 10);
    if (width <= 0 || height <= 0) return 1;
    uint32_t *pixels = calloc((size_t)width * height, sizeof(*pixels));
    if (!pixels || paint_frame_bar(pixels, width, height, id)) return 1;
    for (int y = 0; y < height; y++)
        for (int x = 0; x < width; x++)
            if (putchar(pixels[y * width + x] & 255) == EOF) return 1;
    free(pixels);
    return 0;
}
