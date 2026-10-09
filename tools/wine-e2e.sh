#!/bin/bash
# End-to-end test of Snapr.exe under Wine on a virtual X screen (Linux only).
#
# Needs: wine, xvfb, xdotool, xclip, imagemagick. Build first:
#   cargo build --release --target x86_64-pc-windows-gnu
# Run:
#   xvfb-run -a -s "-screen 0 1280x800x24" tools/wine-e2e.sh
#
# Wine is not Windows: focus and global hotkeys behave differently without a window manager, so
# captures are started from the tray icon here. See TESTING.md for what needs a real Windows PC.
set -u
EXE=${1:-target/x86_64-pc-windows-gnu/release/Snapr.exe}
WORK=$(mktemp -d)
export WINEDEBUG=-all WINEPREFIX=${WINEPREFIX:-$WORK/wineprefix}
TRAY="12 39" # where Wine puts the tray icon without a system tray
failures=0

check() { # description, command...
    local what=$1; shift
    if "$@"; then echo "ok   - $what"; else echo "FAIL - $what"; failures=$((failures + 1)); fi
}
clipboard() { xclip -selection clipboard -t image/png -o > "$1" 2>/dev/null; }
size_is() { [ "$(identify -format '%wx%h' "$1" 2>/dev/null)" = "$2" ]; }
same_pixels() { [ "$(compare -metric AE "$1" "$2" null: 2>&1)" = "0" ]; }

convert -size 1280x800 gradient:'#3a7bd5-#f2994a' -gravity center -pointsize 60 -fill white \
    -annotate 0 'Snapr test 1234' "$WORK/bg.png"
display -window root "$WORK/bg.png" &
wineboot -i >/dev/null 2>&1
sleep 1
wine "$EXE" &
sleep 6
import -window root "$WORK/screen.png"

# Drag a region.
xdotool mousemove $TRAY click 1; sleep 3
xdotool mousemove 300 250 mousedown 1 mousemove 500 300 mousemove 700 450; sleep 1; xdotool mouseup 1; sleep 3
clipboard "$WORK/region.png"
convert "$WORK/screen.png" -crop 400x200+300+250 +repage "$WORK/expected.png"
check "drag copies the region (400x200)" size_is "$WORK/region.png" 400x200
check "region pixels match the screen" same_pixels "$WORK/region.png" "$WORK/expected.png"
xclip -selection clipboard -t image/bmp -o > "$WORK/region.bmp" 2>/dev/null
check "DIB on the clipboard matches too" same_pixels "$WORK/region.bmp" "$WORK/expected.png"

# Enter: all screens.
xdotool mousemove $TRAY click 1; sleep 3; xdotool key Return; sleep 3
clipboard "$WORK/all.png"
check "Enter copies all screens (1280x800)" size_is "$WORK/all.png" 1280x800

# Escape cancels and leaves the clipboard alone.
xdotool mousemove $TRAY click 1; sleep 3; xdotool key Escape; sleep 2
import -window root "$WORK/after-escape.png"
check "Escape closes the overlay" same_pixels \
    <(convert "$WORK/after-escape.png" -crop 300x300+900+450 +repage png:-) \
    <(convert "$WORK/screen.png" -crop 300x300+900+450 +repage png:-)

# A click copies the monitor under the cursor.
echo -n | xclip -selection clipboard
xdotool mousemove $TRAY click 1; sleep 3; xdotool mousemove 640 400 click 1; sleep 3
clipboard "$WORK/click.png"
check "click copies the whole monitor" size_is "$WORK/click.png" 1280x800

wineserver -k
rm -rf "$WORK"
[ $failures -eq 0 ] && echo "all passed" || { echo "$failures failed"; exit 1; }
