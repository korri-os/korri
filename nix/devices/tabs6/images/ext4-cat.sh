# ext4-cat <image> <absolute path>: print a file from an ext4 image without
# mounting it, following symlinks inside the image (debugfs alone does not).
img=$1
path=${2#/}
out=""
hops=0
while [ -n "$path" ]; do
	comp=${path%%/*}
	if [ "$comp" = "$path" ]; then path=""; else path=${path#*/}; fi
	cand="$out/$comp"
	info=$(debugfs -R "stat \"$cand\"" "$img" 2>/dev/null)
	if printf '%s\n' "$info" | grep -q 'Type: symlink'; then
		hops=$((hops + 1))
		[ "$hops" -le 40 ] || { echo "ext4-cat: too many symlinks" >&2; exit 1; }
		dest=$(printf '%s\n' "$info" | sed -n 's/^Fast link dest: "\(.*\)"$/\1/p')
		[ -n "$dest" ] || dest=$(debugfs -R "cat \"$cand\"" "$img" 2>/dev/null)
		case $dest in
		/*) out="" path="${dest#/}${path:+/$path}" ;;
		*) path="$dest${path:+/$path}" ;;
		esac
	else
		[ -n "$info" ] || { echo "ext4-cat: /${cand#/} not found" >&2; exit 1; }
		out=$cand
	fi
done
debugfs -R "cat \"$out\"" "$img" 2>/dev/null
