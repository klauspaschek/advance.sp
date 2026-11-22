#!/bin/sh
# hdmi.sh sync|on|off|plugged|active|status
#
# HDMI out through the RG SP's mini HDMI port (any H700 handheld with one). The display engine
# drives one output at a time, the LCD or the HDMI transmitter. Switching goes through the
# dispdbg debugfs of the sunxi disp2 driver, as NextUI's and the stock firmware's HDMI monitors
# do, and it must happen while no EGL surface exists: the GPU driver latches the framebuffer's
# size as the frontend starts. So launch_frontend.sh runs `hdmi.sh sync` before it starts slot,
# and slot saves the game and ends itself when a cable comes or goes (BaseOS starts it again).
#
#   sync     make the output match the cable: HDMI with a cable in, the LCD without
#   on, off  force one
#   plugged  exit 0 with a cable in
#   active   exit 0 while the HDMI transmitter has the output
#   status   one line for the session log: cable, output, framebuffer and sound route
#
# The signal is 1080p60 whatever the TV is. The display engine scales the framebuffer to the
# whole signal, so on HDMI it is made 16:9 (848x480, a 64-byte aligned row near 853.3) and slot
# draws its 720x480 screen in the middle of it, at its own shape. Without a cable this script
# changes nothing: `sync` on the LCD returns at once.
#
# Ported from Cartridge (github.com/NukeBomB3000/cartridge), GPL-3.0-or-later; the switch was
# found by pvaibhav in the NextUI port for BaseOS.

DBG=${SLOT_HDMI_DBG:-/sys/kernel/debug/dispdbg}
FB=${SLOT_HDMI_FB:-/sys/class/graphics/fb0}
FBDEV=${SLOT_HDMI_FBDEV:-/dev/fb0}
POLL=${SLOT_HDMI_POLL:-0.05}
EXTCON=${SLOT_HDMI_EXTCON:-/sys/class/extcon/hdmi/state}
DISP=${SLOT_HDMI_DISP:-/sys/class/disp/disp/attr/sys}
ROUTE=${SLOT_AUDIO_ROUTE:-/run/slot-audio-device}
HDMI_ROUTE='plughw:CARD=ahubhdmi,DEV=0'
HDMI_PARAM="4 10 0 0 0x4 0x101 0 0 0 8"
LCD_PARAM="1 0"
LCD_SIZE=${SLOT_HDMI_LCD_SIZE:-/run/slot-lcd-size}
TV_SIZE="848 480"

plugged() {
	[ "$(sed -n 's/^HDMI=//p' "$EXTCON" 2>/dev/null)" = 1 ]
}

active() {
	grep -q 'hdmi output' "$DISP" 2>/dev/null
}

# the LCD's size, as the framebuffer had it before the first switch to HDMI
panel_size() {
	if [ -s "$LCD_SIZE" ]; then
		cat "$LCD_SIZE"
		return
	fi
	target="$(sed -n 's/^BASEOS_TARGET=//p' /etc/baseos-release 2>/dev/null)"
	case "$target" in
	rgcubexx) echo "720 720" ;;
	rgsp | rg34xx | rg34xxsp) echo "720 480" ;;
	rg28xx) echo "480 640" ;;
	*) echo "640 480" ;;
	esac
}

fb_size() {
	# $1 width, $2 height: zero first so the driver drops the old allocation
	fbset -fb "$FBDEV" -g 0 0 0 0 32 2>/dev/null
	sleep 0.25
	fbset -fb "$FBDEV" -g "$1" "$2" "$1" "$(($2 * 2))" 32 2>/dev/null
	# the pan offset of the old size is not a page of the new one
	echo 0,0 > "$FB/pan" 2>/dev/null
}

wait_for() {
	# $1 = hdmi | lcd: the attach is asynchronous and slow, up to 3 s
	i=0
	while [ "$i" -lt 60 ]; do
		if [ "$1" = hdmi ]; then active && break; else active || break; fi
		sleep "$POLL"
		i=$((i + 1))
	done
	sleep 0.15
}

# Sound follows the TV unless headphones or USB audio were chosen: an empty route (the speaker)
# becomes the TV's, and only the TV's route is ever cleared again.
audio() {
	current="$(cat "$ROUTE" 2>/dev/null)"
	mkdir -p "$(dirname "$ROUTE")"
	if [ "$1" = on ]; then
		[ -z "$current" ] || return 0
		printf '%s\n' "$HDMI_ROUTE" > "$ROUTE.$$" && mv -f "$ROUTE.$$" "$ROUTE"
	elif [ "$current" = "$HDMI_ROUTE" ]; then
		: > "$ROUTE.$$" && mv -f "$ROUTE.$$" "$ROUTE"
	fi
}

switch() {
	# $1 = hdmi | lcd
	if [ "$1" = hdmi ]; then
		active && { audio on; return 0; }
	else
		active || { audio off; return 0; }
	fi
	# the switch lives in debugfs, which not every boot mounts
	if [ ! -e "$DBG" ] && [ -z "$SLOT_HDMI_DBG" ] && [ -d /sys/kernel/debug ]; then
		mount -t debugfs debugfs /sys/kernel/debug 2>/dev/null
	fi
	[ -w "$DBG/start" ] || { echo "hdmi: no display switch at $DBG" >&2; exit 1; }
	if [ "$1" = hdmi ]; then
		# the LCD's size, once, before the framebuffer is touched for the TV
		if [ ! -s "$LCD_SIZE" ]; then
			vs="$(cat "$FB/virtual_size")"
			echo "${vs%,*} $((${vs#*,} / 2))" > "$LCD_SIZE"
		fi
		set -- hdmi "$HDMI_PARAM" $TV_SIZE
	else
		set -- lcd "$LCD_PARAM" $(panel_size)
	fi
	echo 4 > "$FB/blank"
	cat /dev/zero 2>/dev/null | dd of="$FBDEV" bs=4096 count=900 2>/dev/null
	echo disp0 > "$DBG/name"
	echo switch > "$DBG/command"
	echo "$2" > "$DBG/param"
	echo 1 > "$DBG/start"
	wait_for "$1"
	# the TV's 16:9 framebuffer, or the panel's own size back on the LCD
	fb_size "$3" "$4"
	sleep 0.25
	echo 0 > "$FB/blank"
	if [ "$1" = hdmi ]; then audio on; else audio off; fi
}

case "$1" in
plugged) plugged ;;
active) active ;;
status)
	if plugged; then cable=in; else cable=out; fi
	if active; then output=hdmi; else output=lcd; fi
	echo "hdmi: cable $cable, output $output, fb $(cat "$FB/virtual_size" 2>/dev/null)," \
		"sound '$(cat "$ROUTE" 2>/dev/null)'"
	;;
on) switch hdmi ;;
off) switch lcd ;;
sync)
	if plugged; then switch hdmi; else switch lcd; fi
	;;
*)
	echo "usage: hdmi.sh sync|on|off|plugged|active|status" >&2
	exit 2
	;;
esac
