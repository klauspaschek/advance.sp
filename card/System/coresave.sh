#!/bin/sh
SD="${AGS_CARD:-/mnt/sdcard}"
MOUNTPOINT="${AGS_MOUNTPOINT:-mountpoint}"
DIR="$SD/Logs/crash"
KEEP=5

if ! $MOUNTPOINT -q "$SD" 2>/dev/null; then
	cat > /dev/null
	exit 0
fi

exe="${AGS_CORE_EXE:-$(readlink "/proc/$1/exe" 2>/dev/null)}"
cmd="${AGS_CORE_CMDLINE:-$(tr '\0' ' ' < "/proc/$1/cmdline" 2>/dev/null)}"
mkdir -p "$DIR" 2>/dev/null
echo "$(date '+%F %T') pid=$1 signal=$2 thread=$3 exe=$exe" >> "$DIR/cores.log"

case "$exe ${cmd% }" in
*/advance|*/advance\ *)
	cp "/proc/$1/maps" "$DIR/advance.maps" 2>/dev/null
	cp "$SD/Logs/advance.log" "$DIR/advance-$(date '+%F_%H-%M-%S').log" 2>/dev/null
	ls -1 "$DIR" | grep '^advance-.*\.log$' | sort -r | tail -n +"$((KEEP + 1))" | while read -r old; do
		rm -f "$DIR/$old"
	done
	rm -f "$DIR/advance.core"
	;;
esac
cat > /dev/null
sync
