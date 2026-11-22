#!/bin/sh
# hdmi.sh against a fake sysfs: no cable changes nothing, a cable switches to the TV and routes
# the sound there unless headphones have it, and pulling it switches back.
set -eu

HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/System/hdmi.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT HUP INT TERM

fail() {
	echo "hdmi: $*" >&2
	exit 1
}

setup() {
	rm -rf "$TMP/sys" "$TMP/run"
	mkdir -p "$TMP/sys/dbg" "$TMP/sys/fb0" "$TMP/run"
	: > "$TMP/sys/dbg/start"
	echo "720,960" > "$TMP/sys/fb0/virtual_size"
	: > "$TMP/fb"
	echo "HDMI=${1:-0}" > "$TMP/sys/extcon"
	echo "${2:-lcd output}" > "$TMP/sys/disp"
}

hdmi() {
	SLOT_HDMI_DBG="$TMP/sys/dbg" SLOT_HDMI_FB="$TMP/sys/fb0" SLOT_HDMI_FBDEV="$TMP/fb" \
		SLOT_HDMI_EXTCON="$TMP/sys/extcon" SLOT_HDMI_DISP="$TMP/sys/disp" \
		SLOT_AUDIO_ROUTE="$TMP/run/route" SLOT_HDMI_LCD_SIZE="$TMP/run/lcd-size" \
		SLOT_HDMI_POLL=0 sh "$SCRIPT" "$@"
}

# no cable, on the LCD: nothing is touched
setup 0
hdmi sync || fail "sync without a cable failed"
[ ! -s "$TMP/sys/dbg/start" ] || fail "the display engine was switched without a cable"
[ ! -e "$TMP/run/route" ] || fail "the sound was routed without a cable"
hdmi plugged && fail "a cable was seen that is not there"
hdmi active && fail "HDMI is active on the LCD"

# a cable in: the display engine is told to switch to HDMI, and the sound follows
setup 1
hdmi plugged || fail "the cable was not seen"
hdmi sync || true
[ "$(cat "$TMP/sys/dbg/param")" = "4 10 0 0 0x4 0x101 0 0 0 8" ] || fail "not switched to 1080p HDMI"
[ "$(cat "$TMP/sys/dbg/command")" = switch ] || fail "no switch command"
[ "$(cat "$TMP/run/lcd-size")" = "720 480" ] || fail "the LCD's size was not remembered"
[ "$(cat "$TMP/run/route")" = "plughw:CARD=ahubhdmi,DEV=0" ] || fail "the sound did not follow the TV"

# already on the TV with headphones chosen: they keep the sound
setup 1 "hdmi output mode(10)"
echo "bluealsa:DEV=AA:BB:CC:DD:EE:FF,PROFILE=a2dp" > "$TMP/run/route"
hdmi sync
[ ! -s "$TMP/sys/dbg/start" ] || fail "switched again although HDMI already had the output"
grep -q bluealsa "$TMP/run/route" || fail "the TV took the sound from the headphones"

# the cable pulled while on the TV: back to the LCD, and the TV's sound route is cleared
setup 0 "hdmi output mode(10)"
echo "plughw:CARD=ahubhdmi,DEV=0" > "$TMP/run/route"
echo "720 480" > "$TMP/run/lcd-size"
hdmi sync || true
[ "$(cat "$TMP/sys/dbg/param")" = "1 0" ] || fail "not switched back to the LCD"
[ ! -s "$TMP/run/route" ] || fail "the TV kept the sound after the cable was pulled"

# pulled with headphones chosen: their route stays
setup 0 "hdmi output mode(10)"
echo "bluealsa:DEV=AA:BB:CC:DD:EE:FF,PROFILE=a2dp" > "$TMP/run/route"
hdmi sync || true
grep -q bluealsa "$TMP/run/route" || fail "pulling the cable dropped the headphones"

# status names the cable, the output and the sound route
setup 1 "hdmi output mode(10)"
echo "plughw:CARD=ahubhdmi,DEV=0" > "$TMP/run/route"
status="$(hdmi status)"
[ "$status" = "hdmi: cable in, output hdmi, fb 720,960, sound 'plughw:CARD=ahubhdmi,DEV=0'" ] \
	|| fail "status reads: $status"

# no display switch in the kernel: the failure is said, not silent
setup 1
rm "$TMP/sys/dbg/start"
err="$(hdmi sync 2>&1)" && fail "sync claimed to switch without a display switch"
case "$err" in *"no display switch"*) ;; *) fail "the failure was not said: $err" ;; esac

echo "hdmi: all passed"
