#!/bin/sh
SD="${SLOT_ROOT:-/mnt/sdcard}"
SYS="$SD/System"
RUN="${AGS_RUN:-/run}"
LOGS="$SD/Logs"
SESSION_MAX=65536
export SLOT_ROOT="$SD"
export SLOT_RUN="$RUN"
# An empty file named TRACE_INPUT at the card's root records every input event, and what each
# input device reports, in Logs/input-trace.log: for checking a new controller's buttons.
[ -f "$SD/TRACE_INPUT" ] && export SLOT_TRACE_INPUT=1

# Never inherit networking credentials from a prior launcher session after a
# crash. The Wi-Fi helper only stops supplicants it owns; unrelated Wi-Fi stays
# alone. Reconnection waits for this launcher's credential-vault unlock.
[ ! -f "$SYS/slotsmb.sh" ] || sh "$SYS/slotsmb.sh" stop >/dev/null 2>&1
[ ! -f "$SYS/slotwifi.sh" ] || sh "$SYS/slotwifi.sh" down >/dev/null 2>&1

mkdir -p "$LOGS" 2>/dev/null
for f in slot.log slot.log.1 slot-session.log slot-session.log.1 slot-card.log ags-shutdown-trace.log input-trace.log; do
	[ -f "$SD/$f" ] && mv -f "$SD/$f" "$LOGS/$f"
done
[ -d "$SD/crash" ] && [ ! -e "$LOGS/crash" ] && mv "$SD/crash" "$LOGS/crash"

[ $(wc -c < "$LOGS/slot-session.log" 2>/dev/null || echo 0) -gt "$SESSION_MAX" ] \
	&& mv -f "$LOGS/slot-session.log" "$LOGS/slot-session.log.1"

log() {
	echo "$(date '+%F %T') $*" >> "$LOGS/slot-session.log" 2>/dev/null
}

# Do not run a partially installed tree; retained staging can finish on retry.
if [ -f "$SYS/slotupdate.sh" ]; then
    sh "$SYS/slotupdate.sh" || exit 1
fi

pid="$(cat "$RUN/slot-led.pid" 2>/dev/null)"
if [ -z "$pid" ] || [ ! -d "/proc/$pid" ]; then
	sh "$SYS/led.sh" once
	sh "$SYS/led.sh" loop > /dev/null 2>&1 &
	echo $! > "$RUN/slot-led.pid"
fi

pid="$(cat "$RUN/slot-lid.pid" 2>/dev/null)"
if [ -z "$pid" ] || [ ! -d "/proc/$pid" ]; then
	sh "$SYS/lid.sh" loop > /dev/null 2>&1 &
	echo $! > "$RUN/slot-lid.pid"
fi

echo "|/bin/sh $SYS/coresave.sh %p %s %e %t" > /proc/sys/kernel/core_pattern 2>/dev/null

[ -f "$RUN/slotlink.session" ] && sh "$SYS/slotlink.sh" link down > /dev/null 2>&1

# The output follows the HDMI cable, the TV with one in and the LCD without. It is switched here,
# before slot exists, as the GPU driver latches the framebuffer's size when slot starts; slot
# saves and runs this script again in its own place when the cable changes. The size of the
# last run is not inherited across that. Without a cable and on
# the LCD already, this does nothing.
unset SLOT_FB_SIZE
if [ -f "$SYS/hdmi.sh" ]; then
	sh "$SYS/hdmi.sh" sync >> "$LOGS/slot-session.log" 2>&1 || log "hdmi: the output could not be switched"
	# a line only when HDMI is in play, so a handheld used without a TV logs nothing new
	if sh "$SYS/hdmi.sh" plugged || sh "$SYS/hdmi.sh" active; then
		log "$(sh "$SYS/hdmi.sh" status)"
	fi
	if sh "$SYS/hdmi.sh" active; then
		# the 16:9 framebuffer hdmi.sh made; the TV's mode must not be taken for it
		vs="$(cat /sys/class/graphics/fb0/virtual_size 2>/dev/null)"
		[ -n "$vs" ] && export SLOT_FB_SIZE="${vs%,*}x$((${vs#*,} / 2))"
	fi
fi

[ -f "$LOGS/slot.log" ] && mv -f "$LOGS/slot.log" "$LOGS/slot.log.1"
log "exec $SYS/slot"

exec /lib/ld-linux-aarch64.so.1 "$SYS/slot" > "$LOGS/slot.log" 2>&1
