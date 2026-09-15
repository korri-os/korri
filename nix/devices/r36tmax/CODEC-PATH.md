# Video codec path on Linux 7.2.6

What mainline offers this board, read from the kernel source rather than
inferred. Nothing here is measured on hardware yet; the enumeration run is
still outstanding.

## What the silicon exposes to mainline

`px30.dtsi` gives the VPU a real node with its own IOMMU, which is why
`hantro_vpu` binds on this board:

```
vpu: video-codec@ff442000 {
	compatible = "rockchip,px30-vpu";
	interrupt-names = "vepu", "vdpu";
	iommus = <&vpu_mmu>;
	power-domains = <&power PX30_PD_VPU>;
};
```

The driver's PX30 variant declares the capability set:

```c
const struct hantro_variant px30_vpu_variant = {
	.codec = HANTRO_JPEG_ENCODER | HANTRO_MPEG2_DECODER |
		 HANTRO_VP8_DECODER | HANTRO_H264_DECODER,
```

| Direction | Formats |
| --- | --- |
| Decode | H.264, VP8, MPEG-2 |
| Encode | JPEG only |

**There is no H.264 encoder in mainline for this SoC.** That is a property of
the driver's capability mask, not a configuration gap, so no amount of Kconfig
or device-tree work in this tree produces one. Hardware-encoded Sunshine needs
the vendor VEPU2 path, which is quarantined and unsafe to deploy; read
`.pi-web/handoffs/a3299ec1-9f0d-47a6-96c0-49d08ab29fba.md` before touching it.

Decode is the part worth having, and it is the part nobody has exercised.

## Configuration in this tree

`dts/config` selects the driver and its stateless helpers:

```
CONFIG_VIDEO_HANTRO=m
CONFIG_VIDEO_HANTRO_ROCKCHIP=y
CONFIG_V4L2_H264=m
CONFIG_V4L2_VP9=m
CONFIG_V4L2_JPEG_HELPER=m
```

On the running device `hantro_vpu`, `v4l2_jpeg`, `v4l2_vp9` and `v4l2_h264`
were all loaded, so the driver bound and pulled its helpers in.

`CONFIG_VIDEO_ROCKCHIP_VDEC=m` is also set, inherited from the ROCKNIX seed.
PX30 has no rkvdec node, so that module has nothing to bind to here. It is
harmless, not evidence of a second decoder.

## Outstanding

Enumeration has not run: the device was wedged by a Wi-Fi reload test when this
was written. `/tmp/r36tmax-codec-test.sh` reports the v4l2 nodes, their
advertised output and capture formats, the VPU power domain and which userspace
decoders exist. It runs no codec job.

After that, the open questions are whether a real H.264 decode completes
through the stateless V4L2 request interface, and whether any userspace on this
image can drive it. Stateless decoding needs explicit support; a normal FFmpeg
or Chromium build will fall back to software and show nothing but CPU load.
Measure temperature during any decode test: this board idles near 55 C and the
kiosk already pushes it past 80 C.
