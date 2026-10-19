# fex-unpack <work-dir> <squashfs> <overlay>...
#
# Unpack the verified FEX x86 rootfs files that korrid downloaded, in the
# order plugin.ts declares them: the ArchLinux squashfs first, then the Arch
# package overlays extracted over it. korrid owns the download, the hash
# check, the work directory, the read-only flip, the sync and the rename to
# the ready directory. This program writes only under <work-dir>.
#
# Exit status: 0 ready, 1 failed. Diagnostics go to stderr; stdout carries
# nothing korrid reads.
set -Eeuo pipefail
umask 022

say() { printf 'fex-unpack: %s\n' "$*" >&2; }
fail() { say "$*"; exit 1; }
trap 'fail "failed at line $LINENO (status $?)"' ERR
trap 'exit 130' INT
trap 'exit 143' TERM

(( $# >= 2 )) || fail 'usage: fex-unpack <work-dir> <squashfs> <overlay>...'
work="$1"
shift
[[ "$work" == /* && -d "$work" ]] || fail "work directory is not an absolute directory: $work"
for file in "$@"; do
  [[ -f "$file" ]] || fail "not a file: $file"
done
base="$1"
shift

say "unpacking rootfs into $work"
unsquashfs -no-progress -no-xattrs -f -d "$work" "$base" >&2
# The image carries read-only directories; the overlays write into them.
chmod -R u+w "$work"
for overlay in "$@"; do
  tar --zstd --no-same-owner \
    --exclude='.BUILDINFO' --exclude='.CHANGELOG' --exclude='.INSTALL' \
    --exclude='.MTREE' --exclude='.PKGINFO' -xf "$overlay" -C "$work"
done

for file in usr/lib/libgallium-26.1.2-arch1.1.so usr/lib32/libgallium-26.1.2-arch1.1.so \
  usr/share/vulkan/icd.d/freedreno_icd.x86_64.json usr/share/vulkan/icd.d/freedreno_icd.i686.json \
  usr/lib/libdisplay-info.so.3 usr/lib32/libdisplay-info.so.3 \
  usr/lib/libxcb-keysyms.so.1 usr/lib32/libxcb-keysyms.so.1; do
  [[ -f "$work/$file" ]] || fail "rootfs is missing $file"
done
case "$(od -An -tx1 -j18 -N2 "$work/usr/lib/libvulkan_freedreno.so")" in
  *"3e 00"*) ;; *) fail 'Freedreno ICD is not x86_64' ;;
esac
case "$(od -An -tx1 -j18 -N2 "$work/usr/lib32/libvulkan_freedreno.so")" in
  *"03 00"*) ;; *) fail 'Freedreno ICD is not i686' ;;
esac
