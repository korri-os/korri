obj-m := rk_vcodec.o rk_mpp_overlay.o
rk_vcodec-y := mpp_service.o mpp_common.o mpp_iommu.o mpp_rkvenc.o
ccflags-y += -I$(src) -DMPP_VERSION='"rockchip-6.12-470f9dcc-rg353m"'
ccflags-y += -DCONFIG_ROCKCHIP_MPP_RKVENC=1 -DCONFIG_ROCKCHIP_MPP_PROC_FS=1
