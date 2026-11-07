cmake_minimum_required(VERSION 3.19)
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
project(korri_webos_media_link_spike LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_SKIP_RPATH ON)
find_package(PkgConfig REQUIRED)
pkg_check_modules(MEDIA REQUIRED IMPORTED_TARGET libplayerAPIs AcbAPI pbnjson_c glib-2.0)

# Build the pinned protocol library normally. Its pinned ENet gitlink is
# populated by Nix, not downloaded by CMake or by anything on the target.
add_subdirectory("${COMMON_SOURCE}" common)

# Select only the webOS 3 Starfish implementation, deliberately excluding
# upstream's SDL backends, NDL executable-memory workaround and module chooser.
set(SS4S_CORE "${SS4S_SOURCE}/src")
add_library(ss4s SHARED
    "${SS4S_CORE}/library.c" "${SS4S_CORE}/driver.c"
    "${SS4S_CORE}/player.c" "${SS4S_CORE}/audio.c"
    "${SS4S_CORE}/video.c" "${SS4S_CORE}/module.c"
    "${SS4S_CORE}/stats.c" "${SS4S_CORE}/module_libdl.c"
    "${SS4S_CORE}/mutex_pthread.c")
target_include_directories(ss4s PUBLIC "${SS4S_SOURCE}/include"
    PRIVATE "${SS4S_SOURCE}/modules/interface/include" "${SS4S_CORE}")
target_compile_definitions(ss4s PRIVATE SS4S_LIBRARY_SUFFIX=".so")
target_compile_options(ss4s PRIVATE -Wall -Wextra -Werror)
target_link_libraries(ss4s PRIVATE dl pthread rt)
target_link_options(ss4s PRIVATE -Wl,--no-undefined)

set(SMP "${SS4S_SOURCE}/modules/webos/smp")
add_library(ss4s-smp-webos3 SHARED
    "${SMP}/src/smp_module.c"
    "${SMP}/src/smp_player.c"
    "${SMP}/src/smp_video.c"
    "${SMP}/src/smp_audio.c"
    "${SMP}/src/smp_resource_acb.c"
    "${SMP}/src/pbnjson_compat.c"
    "${SMP}/wrapper/StarfishMediaAPIs_C.cpp"
    "${SS4S_SOURCE}/modules/webos/utils/jail_check.c")
target_include_directories(ss4s-smp-webos3 PRIVATE
    "${SS4S_SOURCE}/include"
    "${SS4S_SOURCE}/modules/interface/include"
    "${SS4S_SOURCE}/modules/webos/utils"
    "${SMP}/src" "${SMP}/wrapper")
target_compile_definitions(ss4s-smp-webos3 PRIVATE
    _GNU_SOURCE _GLIBCXX_USE_CXX11_ABI=0
    SS4S_MODULE_NAME="smp-webos3"
    SS4S_MODULE_ENTRY=SS4S_ModuleOpen_SMP_WEBOS3
    SS4S_MODULE_CHECK=SS4S_ModuleCheck_SMP_WEBOS3 SS4S_JAIL_CHECK=1)
target_link_libraries(ss4s-smp-webos3 PRIVATE PkgConfig::MEDIA dl pthread)
# Resolve the vendor API at link time. A build against SDK stubs is not proof
# that this TV has the same ABI; the dependency report is part of the output.
target_link_options(ss4s-smp-webos3 PRIVATE -Wl,--no-undefined)
set_target_properties(ss4s-smp-webos3 PROPERTIES PREFIX "")

add_executable(media-link-probe media-link-probe.c)
target_include_directories(media-link-probe PRIVATE "${SS4S_SOURCE}/include"
    "${SS4S_SOURCE}/modules/interface/include")
target_link_libraries(media-link-probe PRIVATE moonlight-common-c ss4s ss4s-smp-webos3)
target_compile_options(media-link-probe PRIVATE -Wall -Wextra -Werror)

install(TARGETS moonlight-common-c ss4s ss4s-smp-webos3 media-link-probe
    LIBRARY DESTINATION lib RUNTIME DESTINATION bin)
