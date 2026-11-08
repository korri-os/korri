cmake_minimum_required(VERSION 3.19)
set(CMAKE_POLICY_VERSION_MINIMUM 3.5)
project(korri_webos_media_test LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 11)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(CMAKE_SKIP_RPATH ON)

# Select only the approved webOS 4 NDL implementation; no backend chooser.
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

# Product multi-call executable: reuse the tested CLI without another exe path.
function(add_launcher driver)
    add_library(korri-media-test OBJECT media-test.c)
    target_compile_definitions(korri-media-test PRIVATE main=korri_media_test_main MEDIA_DRIVER="${driver}")
    target_include_directories(korri-media-test PRIVATE "${SS4S_SOURCE}/include")
    target_compile_options(korri-media-test PRIVATE -Wall -Wextra -Werror)
    add_executable(korri-webos "${KORRI_WEBOS_LAUNCHER}"
        $<TARGET_OBJECTS:korri-media-test>)
    target_compile_definitions(korri-webos PRIVATE KORRI_MEDIA_TEST)
    target_compile_options(korri-webos PRIVATE -Wall -Wextra -Werror)
    target_link_libraries(korri-webos PRIVATE ss4s rt)
    # Package-relative lookup only. Core dlopen also needs the module beside it.
    set(CMAKE_SKIP_RPATH OFF PARENT_SCOPE)
    set_target_properties(media-test PROPERTIES SKIP_BUILD_RPATH TRUE)
    set_target_properties(korri-webos PROPERTIES
        SKIP_BUILD_RPATH TRUE INSTALL_RPATH "$ORIGIN/../lib")
    set_target_properties(ss4s PROPERTIES
        SKIP_BUILD_RPATH TRUE INSTALL_RPATH "$ORIGIN")
    install(TARGETS korri-webos RUNTIME DESTINATION bin)
endfunction()

option(MEDIA_HOST_TEST "Build native reference-sink CLI, never ship it" OFF)
if(MEDIA_HOST_TEST)
    set(DUMMY "${SS4S_SOURCE}/modules/dummy")
    add_library(ss4s-dummy SHARED
        "${DUMMY}/dummy_module.c" "${DUMMY}/dummy_player.c"
        "${DUMMY}/dummy_video.c" "${DUMMY}/dummy_audio.c")
    target_include_directories(ss4s-dummy PRIVATE
        "${SS4S_SOURCE}/include" "${SS4S_SOURCE}/modules/interface/include")
    target_link_libraries(ss4s-dummy PRIVATE pthread)
    set_target_properties(ss4s-dummy PROPERTIES PREFIX "")
    add_executable(media-test media-test.c)
    target_compile_definitions(media-test PRIVATE MEDIA_DRIVER="dummy")
    target_compile_options(media-test PRIVATE -Wall -Wextra -Werror)
    target_link_libraries(media-test PRIVATE ss4s rt)
    if(KORRI_WEBOS_LAUNCHER)
        add_launcher(dummy)
    endif()
    return()
endif()

find_path(NDL_DIRECTMEDIA_INCLUDE_DIR NAMES NDL_directmedia.h
    PATH_SUFFIXES libndl-media REQUIRED)
find_library(NDL_DIRECTMEDIA_LIBRARY NAMES NDL_directmedia REQUIRED)
set(NDL "${SS4S_SOURCE}/modules/webos/ndl/webos4")
add_library(ss4s-ndl-webos4 SHARED
    "${NDL}/ndl_module.c" "${NDL}/ndl_player.c"
    "${NDL}/ndl_video.c" "${NDL}/ndl_audio.c"
    "${SS4S_SOURCE}/modules/webos/utils/jail_check.c"
    "${SS4S_SOURCE}/modules/webos/utils/m3_kadp_fix.c"
    "${SS4S_SOURCE}/modules/webos/utils/max_res.c")
target_include_directories(ss4s-ndl-webos4 PRIVATE
    "${SS4S_SOURCE}/include" "${SS4S_SOURCE}/modules/interface/include"
    "${SS4S_SOURCE}/modules/webos/utils" "${NDL}" "${NDL_DIRECTMEDIA_INCLUDE_DIR}")
target_compile_definitions(ss4s-ndl-webos4 PRIVATE _GNU_SOURCE)
target_link_libraries(ss4s-ndl-webos4 PRIVATE "${NDL_DIRECTMEDIA_LIBRARY}" pthread dl)
target_link_options(ss4s-ndl-webos4 PRIVATE -Wl,--no-undefined)
set_target_properties(ss4s-ndl-webos4 PROPERTIES PREFIX "")

add_executable(media-test media-test.c)
target_link_libraries(media-test PRIVATE ss4s rt)
target_compile_options(media-test PRIVATE -Wall -Wextra -Werror)

install(TARGETS ss4s ss4s-ndl-webos4 media-test
    LIBRARY DESTINATION lib RUNTIME DESTINATION bin)


if(KORRI_WEBOS_LAUNCHER)
    add_launcher(ndl-webos4)
endif()
