// SPDX-License-Identifier: GPL-2.0
/* Direct V4L2 acceptance: fixed QP, profiles, format alignment and empty drain. */
#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <linux/videodev2.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <unistd.h>

static void check(int ok, const char *operation)
{
	if (!ok) {
		perror(operation);
		exit(1);
	}
}

static void call(int fd, unsigned long request, void *arg, const char *name)
{
	int ret;

	do {
		ret = ioctl(fd, request, arg);
	} while (ret < 0 && errno == EINTR);
	check(ret == 0, name);
}

static void control(int fd, uint32_t id, int value)
{
	struct v4l2_control c = { .id = id, .value = value };

	call(fd, VIDIOC_S_CTRL, &c, "S_CTRL");
	if (id == V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME)
		return;
	call(fd, VIDIOC_G_CTRL, &c, "G_CTRL");
	check(c.value == value, "control readback");
}

struct buffer {
	void *data;
	unsigned int size;
};

static struct buffer allocate(int fd, enum v4l2_buf_type type)
{
	struct v4l2_requestbuffers req = {
		.count = type == V4L2_BUF_TYPE_VIDEO_OUTPUT ? 16 : 4,
		.type = type, .memory = V4L2_MEMORY_MMAP,
	};
	struct v4l2_buffer b = {
		.type = type, .memory = V4L2_MEMORY_MMAP, .index = 0,
	};
	struct buffer result;
	struct v4l2_create_buffers extra = {
		.count = 1, .memory = V4L2_MEMORY_MMAP,
		.format = { .type = type },
	};

	call(fd, VIDIOC_REQBUFS, &req, "REQBUFS");
	check(req.count >= 1, "buffer count");
	printf("queue type=%u allocated=%u\n", type, req.count);
	/* Exercise FFmpeg's 16 OUTPUT / 4 CAPTURE request and the pool cap. */
	check(req.count <= (type == V4L2_BUF_TYPE_VIDEO_OUTPUT ? 8u : 4u),
	      "bounded buffer count");
	call(fd, VIDIOC_G_FMT, &extra.format, "G_FMT for CREATE_BUFS");
	errno = 0;
	check(ioctl(fd, VIDIOC_CREATE_BUFS, &extra) == -1 && errno == ENOBUFS,
	      "CREATE_BUFS at queue cap");
	call(fd, VIDIOC_QUERYBUF, &b, "QUERYBUF");
	result.size = b.length;
	result.data = mmap(NULL, b.length, PROT_READ | PROT_WRITE, MAP_SHARED, fd, b.m.offset);
	check(result.data != MAP_FAILED, "mmap");
	return result;
}

static void queue(int fd, enum v4l2_buf_type type, unsigned int bytes, unsigned int n)
{
	struct v4l2_buffer b = {
		.type = type, .memory = V4L2_MEMORY_MMAP, .index = 0,
		.bytesused = bytes, .field = V4L2_FIELD_NONE,
		.timestamp = { .tv_sec = n / 60, .tv_usec = n % 60 * 1000000 / 60 },
	};

	call(fd, VIDIOC_QBUF, &b, "QBUF");
}

static struct v4l2_buffer dequeue(int fd, enum v4l2_buf_type type)
{
	struct v4l2_buffer b = { .type = type, .memory = V4L2_MEMORY_MMAP };

	call(fd, VIDIOC_DQBUF, &b, "DQBUF");
	return b;
}

static void pattern(uint8_t *p, unsigned int w, unsigned int h,
		    unsigned int stride, unsigned int storage_h, unsigned int n)
{
	memset(p, 16, stride * storage_h);
	for (unsigned int y = 0; y < h; y++)
		for (unsigned int x = 0; x < w; x++)
			p[y * stride + x] = 16 + (x + y + n * 4) % 220;
	memset(p + stride * storage_h, 128, stride * storage_h / 2);
}

static void queue_frame(int fd, struct buffer src, unsigned int w, unsigned int h,
			unsigned int stride, unsigned int storage_h,
			unsigned int raw_size, unsigned int n)
{
	pattern(src.data, w, h, stride, storage_h, n);
	if (n % 30 == 0)
		control(fd, V4L2_CID_MPEG_VIDEO_FORCE_KEY_FRAME, 0);
	queue(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE, 0, 0);
	queue(fd, V4L2_BUF_TYPE_VIDEO_OUTPUT, raw_size, n);
}

static void finish_frame(int fd, struct buffer dst, FILE *out, int expect_error)
{
	struct pollfd pfd = { .fd = fd, .events = POLLIN };

	check(poll(&pfd, 1, 2000) > 0, "poll encode");
	struct v4l2_buffer b = dequeue(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE);
	struct v4l2_buffer src = dequeue(fd, V4L2_BUF_TYPE_VIDEO_OUTPUT);

	check(!!(b.flags & V4L2_BUF_FLAG_ERROR) == expect_error, "capture error flag");
	check(!!(src.flags & V4L2_BUF_FLAG_ERROR) == expect_error, "output error flag");
	check(!(b.flags & V4L2_BUF_FLAG_LAST), "unexpected LAST on frame");
	if (expect_error) {
		check(b.bytesused == 0, "failed frame payload");
		puts("TIMEOUT-BUFFERS-PASS capture_error=1 output_error=1 bytes=0");
		return;
	}
	check(b.flags & V4L2_BUF_FLAG_KEYFRAME, "IDR flag");
	check(b.bytesused && b.bytesused <= dst.size, "coded size");
	check(fwrite(dst.data, 1, b.bytesused, out) == b.bytesused, "write");
}

static void drain(int fd)
{
	struct v4l2_encoder_cmd cmd = { .cmd = V4L2_ENC_CMD_STOP };
	struct pollfd pfd = { .fd = fd, .events = POLLIN | POLLPRI };
	struct v4l2_event ev;

	/* No source or capture buffer remains queued: exercise next_buf_last. */
	call(fd, VIDIOC_ENCODER_CMD, &cmd, "ENC_CMD_STOP");
	queue(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE, 0, 0);
	check(poll(&pfd, 1, 2000) > 0, "poll LAST");
	struct v4l2_buffer last = dequeue(fd, V4L2_BUF_TYPE_VIDEO_CAPTURE);

	check((last.flags & V4L2_BUF_FLAG_LAST) && !last.bytesused, "empty LAST");
	call(fd, VIDIOC_DQEVENT, &ev, "DQEVENT EOS");
	check(ev.type == V4L2_EVENT_EOS, "EOS type");
}

int main(int argc, char **argv)
{
	check(argc == 7, "usage: controls-probe width height qp profile frames output");
	unsigned int w = strtoul(argv[1], NULL, 10), h = strtoul(argv[2], NULL, 10);
	int qp = atoi(argv[3]), profile = atoi(argv[4]), frames = atoi(argv[5]);
	FILE *out = fopen(argv[6], "wb");
	int fd = open("/dev/video1", O_RDWR | O_NONBLOCK);
	struct v4l2_format f = {
		.type = V4L2_BUF_TYPE_VIDEO_OUTPUT,
		.fmt.pix = { .width = w, .height = h, .pixelformat = V4L2_PIX_FMT_NV12,
			     .field = V4L2_FIELD_NONE },
	};
	struct v4l2_event_subscription sub = { .type = V4L2_EVENT_EOS };
	struct v4l2_queryctrl gop = { .id = V4L2_CID_MPEG_VIDEO_GOP_SIZE };
	struct v4l2_encoder_cmd cmd = { .cmd = V4L2_ENC_CMD_STOP };
	const char *test = getenv("PROBE_CASE");
	int all = !test || !strcmp(test, "all");
	int start_test = all || !strcmp(test, "start");
	int streamon_test = all || !strcmp(test, "streamon");
	int extra_frames = 0;
	enum v4l2_buf_type src_type = V4L2_BUF_TYPE_VIDEO_OUTPUT;
	enum v4l2_buf_type dst_type = V4L2_BUF_TYPE_VIDEO_CAPTURE;

	check(out != NULL && fd >= 0, "open");
	call(fd, VIDIOC_QUERYCTRL, &gop, "query GOP");
	check(gop.minimum == 1 && gop.maximum == 1, "intra-only GOP");
	control(fd, V4L2_CID_MPEG_VIDEO_FRAME_RC_ENABLE, 0);
	control(fd, V4L2_CID_MPEG_VIDEO_H264_I_FRAME_QP, qp);
	control(fd, V4L2_CID_MPEG_VIDEO_H264_PROFILE, profile);
	control(fd, V4L2_CID_MPEG_VIDEO_H264_LEVEL, V4L2_MPEG_VIDEO_H264_LEVEL_4_2);
	control(fd, V4L2_CID_MPEG_VIDEO_HEADER_MODE,
		V4L2_MPEG_VIDEO_HEADER_MODE_JOINED_WITH_1ST_FRAME);
	control(fd, V4L2_CID_MPEG_VIDEO_PREPEND_SPSPPS_TO_IDR, 1);
	call(fd, VIDIOC_S_FMT, &f, "S_FMT OUTPUT");
	unsigned int stride = f.fmt.pix.bytesperline, storage_h = f.fmt.pix.height;
	unsigned int raw_size = f.fmt.pix.sizeimage;
	printf("FORMAT visible=%ux%u storage=%ux%u stride=%u bytes=%u\n",
	       w, h, f.fmt.pix.width, storage_h, stride, raw_size);
	if (w != f.fmt.pix.width || h != storage_h) {
		struct v4l2_selection sel = {
			.type = src_type, .target = V4L2_SEL_TGT_CROP,
			.r = { .width = w, .height = h },
		};

		call(fd, VIDIOC_S_SELECTION, &sel, "S_SELECTION");
	}
	f.type = dst_type;
	f.fmt.pix.pixelformat = V4L2_PIX_FMT_H264;
	call(fd, VIDIOC_S_FMT, &f, "S_FMT CAPTURE");
	call(fd, VIDIOC_SUBSCRIBE_EVENT, &sub, "subscribe EOS");
	struct buffer src = allocate(fd, src_type), dst = allocate(fd, dst_type);
	if (all)
		call(fd, VIDIOC_ENCODER_CMD, &cmd, "STOP before both STREAMON");
	call(fd, VIDIOC_STREAMON, &dst_type, "STREAMON CAPTURE");
	if (all) {
		call(fd, VIDIOC_ENCODER_CMD, &cmd, "STOP before OUTPUT STREAMON");
		puts("STOP-BEFORE-STREAMON-PASS queues=both-off,capture-only");
	}
	call(fd, VIDIOC_STREAMON, &src_type, "STREAMON OUTPUT");
	if (getenv("CLOSE_PENDING_TIMEOUT")) {
		queue_frame(fd, src, w, h, stride, storage_h, raw_size, 0);
		check(close(fd) == 0, "close pending job");
		puts("CLOSE-PENDING-PASS");
		return 0;
	}
	if (getenv("EXPECT_TIMEOUT")) {
		queue_frame(fd, src, w, h, stride, storage_h, raw_size, 0);
		finish_frame(fd, dst, out, 1);
	}
	for (int n = 0; n < frames; n++) {
		queue_frame(fd, src, w, h, stride, storage_h, raw_size, n);
		finish_frame(fd, dst, out, 0);
	}
	if (start_test) {
		drain(fd);
		queue_frame(fd, src, w, h, stride, storage_h, raw_size, frames);
		cmd.cmd = V4L2_ENC_CMD_START;
		call(fd, VIDIOC_ENCODER_CMD, &cmd, "ENC_CMD_START");
		finish_frame(fd, dst, out, 0);
		extra_frames++;
		puts("START-RESTART-PASS queued-before-resume=1 encoded-and-dequeued=1");
	}
	if (streamon_test) {
		drain(fd);
		call(fd, VIDIOC_STREAMOFF, &src_type, "restart STREAMOFF OUTPUT");
		queue_frame(fd, src, w, h, stride, storage_h, raw_size,
			    frames + extra_frames);
		call(fd, VIDIOC_STREAMON, &src_type, "restart STREAMON OUTPUT");
		finish_frame(fd, dst, out, 0);
		extra_frames++;
		puts("STREAMON-RESTART-PASS queued-before-resume=1 encoded-and-dequeued=1");
	}
	call(fd, VIDIOC_STREAMOFF, &src_type, "STREAMOFF OUTPUT");
	call(fd, VIDIOC_STREAMOFF, &dst_type, "STREAMOFF CAPTURE");
	munmap(src.data, src.size);
	munmap(dst.data, dst.size);
	close(fd);
	fclose(out);
	printf("CONTROLS-PASS frames=%d qp=%d profile=%d empty_LAST=1 EOS=1 start=%d streamon=%d\n",
	       frames + extra_frames, qp, profile, start_test, streamon_test);
	return 0;
}
