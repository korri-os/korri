#include <libavcodec/avcodec.h>
#include <libavutil/error.h>
#include <libavutil/opt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void checked(int result, const char *operation) {
    if (result < 0) {
        char message[AV_ERROR_MAX_STRING_SIZE];
        av_strerror(result, message, sizeof(message));
        fprintf(stderr, "%s: %s\n", operation, message);
        exit(1);
    }
}

static int drain(AVCodecContext *ctx, AVPacket *packet, FILE *output, int *keys) {
    int result;
    while ((result = avcodec_receive_packet(ctx, packet)) >= 0) {
        if (fwrite(packet->data, 1, packet->size, output) != (size_t)packet->size)
            exit(1);
        printf("packet pts=%ld size=%d key=%d\n", (long)packet->pts,
               packet->size, !!(packet->flags & AV_PKT_FLAG_KEY));
        *keys += !!(packet->flags & AV_PKT_FLAG_KEY);
        av_packet_unref(packet);
    }
    if (result != AVERROR(EAGAIN) && result != AVERROR_EOF)
        checked(result, "receive");
    return result;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fprintf(stderr, "usage: encode codec width height output\n");
        return 2;
    }
    const AVCodec *codec = avcodec_find_encoder_by_name(argv[1]);
    if (!codec) return 3;
    AVCodecContext *ctx = avcodec_alloc_context3(codec);
    AVFrame *frame = av_frame_alloc();
    AVPacket *packet = av_packet_alloc();
    if (!ctx || !frame || !packet) return 4;
    ctx->width = atoi(argv[2]);
    ctx->height = atoi(argv[3]);
    ctx->pix_fmt = AV_PIX_FMT_NV12;
    ctx->time_base = (AVRational){1, 60};
    ctx->framerate = (AVRational){60, 1};
    ctx->bit_rate = 10000000;
    ctx->gop_size = 1000;
    ctx->max_b_frames = 0;
    checked(av_opt_set_int(ctx->priv_data, "repeat_headers", 1, 0), "repeat headers");
    checked(avcodec_open2(ctx, codec, NULL), "open");
    FILE *output = fopen(argv[4], "wb");
    if (!output) return 5;
    int keys = 0;
    for (int i = 0; i < 120; i++) {
        av_frame_unref(frame);
        frame->width = ctx->width;
        frame->height = ctx->height;
        frame->format = ctx->pix_fmt;
        checked(av_frame_get_buffer(frame, 1), "allocate");
        for (int y = 0; y < frame->height; y++)
            for (int x = 0; x < frame->width; x++)
                frame->data[0][y * frame->linesize[0] + x] = 16 + ((x + y + i * 4) % 220);
        for (int y = 0; y < frame->height / 2; y++)
            memset(frame->data[1] + y * frame->linesize[1], 128, frame->width);
        frame->pts = i;
        frame->pict_type = i % 30 == 0 ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_NONE;
        if (i % 30 == 0) frame->flags |= AV_FRAME_FLAG_KEY;
        checked(avcodec_send_frame(ctx, frame), "send");
        drain(ctx, packet, output, &keys);
    }
    checked(avcodec_send_frame(ctx, NULL), "flush");
    int result = drain(ctx, packet, output, &keys);
    fclose(output);
    printf("SUMMARY codec=%s width=%d height=%d key_packets=%d eof=%d\n",
           argv[1], ctx->width, ctx->height, keys, result == AVERROR_EOF);
    av_frame_free(&frame);
    av_packet_free(&packet);
    avcodec_free_context(&ctx);
    return result == AVERROR_EOF ? 0 : 6;
}
