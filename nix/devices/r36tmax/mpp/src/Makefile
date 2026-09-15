# The overlay loader is its own module so a failed device-tree apply cannot
# take the codec driver down with it.
obj-m := rk_vcodec.o rk_vepu_overlay.o
rk_vcodec-y := mpp_service.o mpp_common.o mpp_iommu.o mpp_vepu2.o
ccflags-y += -I$(src) -DMPP_VERSION='"rockchip-6.12-470f9dcc-r36tmax"'
ccflags-y += -DCONFIG_ROCKCHIP_MPP_VEPU2=1 -DCONFIG_ROCKCHIP_MPP_PROC_FS=1
