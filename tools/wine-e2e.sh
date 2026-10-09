#!/bin/bash
# End-to-end test of Snapr.exe under Wine on a virtual X screen (Linux only).
#
# Needs: wine, xvfb, xdotool, xclip, imagemagick. Build first:
#   cargo build --release --target x86_64-pc-windows-gnu
# Run:
#   xvfb-run -a -s "-screen 0 1280x800x24" tools/wine-e2e.sh
#
# Wine is not Windows: focus and global hotkeys behave differently without a window manager, so
# captures are started from the tray icon here, and keys go to the window under the mouse. See TESTING.md for what needs a real Windows PC.
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
different_pixels() { ! same_pixels "$1" "$2"; }

convert -size 1280x800 gradient:'#3a7bd5-#f2994a' -gravity center -pointsize 60 -fill white \
    -annotate 0 'Snapr test 1234' "$WORK/bg.png"
display -window root "$WORK/bg.png" &
wineboot -i >/dev/null 2>&1
sleep 1
wine "$EXE" &
# A fresh Wine prefix needs a while before the tray icon is there.
sleep 12
import -window root "$WORK/screen.png"

# Every capture opens the editor; Esc closes it (without changes nothing is copied again).
# Without a window manager X sends keys to the window under the mouse, so point at the editor
# (which is centred) first. On Windows the editor simply gets the focus.
close_editor() { xdotool mousemove 640 400; sleep 0.5; xdotool key Escape; sleep 2; }

# Drag a region.
xdotool mousemove $TRAY click 1; sleep 3
xdotool mousemove 300 250 mousedown 1 mousemove 500 300 mousemove 700 450; sleep 1; xdotool mouseup 1; sleep 4
clipboard "$WORK/region.png"
convert "$WORK/screen.png" -crop 400x200+300+250 +repage "$WORK/expected.png"
check "drag copies the region (400x200)" size_is "$WORK/region.png" 400x200
check "region pixels match the screen" same_pixels "$WORK/region.png" "$WORK/expected.png"
xclip -selection clipboard -t image/bmp -o > "$WORK/region.bmp" 2>/dev/null
check "DIB on the clipboard matches too" same_pixels "$WORK/region.bmp" "$WORK/expected.png"

# The editor shows the 400x200 image 1:1 at its minimum size, centred: image at (440,324).
import -window root "$WORK/editor.png"
check "editor shows the capture 1:1" same_pixels \
    <(convert "$WORK/editor.png" -crop 400x200+440+324 +repage png:-) "$WORK/expected.png"

# Draw a red arrow, undo, redo; closing puts the edited image on the clipboard.
xdotool mousemove 470 360 mousedown 1 mousemove 600 420 mousemove 700 470; sleep 0.5; xdotool mouseup 1; sleep 1
xdotool key ctrl+z; sleep 0.5; xdotool key ctrl+y; sleep 0.5
close_editor
clipboard "$WORK/edited.png"
pixel() { convert "$1" -crop 1x1+$2+$3 -format '%[pixel:p]' info:; }
check "closing copies the image with the arrow" [ "$(pixel "$WORK/edited.png" 145 91)" = "srgb(255,59,48)" ]

# A number marker and a pixelated region (image origin is at screen 440,324).
xdotool mousemove $TRAY click 1; sleep 3
xdotool mousemove 300 250 mousedown 1 mousemove 500 300 mousemove 700 450; sleep 1; xdotool mouseup 1; sleep 4
xdotool key n; sleep 0.3; xdotool mousemove 470 350 click 1; sleep 0.5
xdotool key p; sleep 0.3
xdotool mousemove 560 445 mousedown 1 mousemove 620 480 mousemove 700 505; sleep 0.5; xdotool mouseup 1; sleep 1
close_editor
clipboard "$WORK/marked.png"
check "closing copies the number marker" [ "$(pixel "$WORK/marked.png" 22 20)" = "srgb(255,59,48)" ]
check "pixelated blocks are uniform" [ "$(pixel "$WORK/marked.png" 127 127)" = "$(pixel "$WORK/marked.png" 133 133)" ]
check "pixelating changed the image" different_pixels \
    <(convert "$WORK/marked.png" -crop 140x60+120+121 +repage png:-) \
    <(convert "$WORK/expected.png" -crop 140x60+120+121 +repage png:-)

# Saving with the dialog's defaults writes a PNG to Pictures\Screenshots.
xdotool mousemove $TRAY click 1; sleep 3
xdotool mousemove 300 250 mousedown 1 mousemove 700 450; sleep 1; xdotool mouseup 1; sleep 4
xdotool key ctrl+s; sleep 4; xdotool key Return; sleep 3
SAVED=$(find "$WINEPREFIX/drive_c/users" -path "*Screenshots/Snapr *.png" | head -1)
check "Ctrl+S saves a PNG" size_is "$SAVED" 400x200
close_editor

# Enter: all screens. The editor scales it down to fit the work area.
xdotool mousemove $TRAY click 1; sleep 3; xdotool key Return; sleep 4
clipboard "$WORK/all.png"
check "Enter copies all screens (1280x800)" size_is "$WORK/all.png" 1280x800
EDITOR=$(xdotool search --name "Snapr \(Rust\)" | tail -1)
read -r W H < <(xdotool getwindowgeometry "$EDITOR" | awk '/Geometry/ { split($2, s, "x"); print s[1], s[2] }')
check "large captures fit in the editor ($W x $H)" [ "$W" -le 1178 ] && [ "$H" -le 736 ]
close_editor

# Escape cancels and leaves the screen as it was.
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
close_editor

# Settings: tray menu › Settings…, record Ctrl+Alt+K, tick "Start with Windows", Save.
# The window (454x322 client) is centred: shortcut field at 550,307, checkbox at 450,493,
# Save at 800,538.
xdotool mousemove $TRAY click 3; sleep 1; xdotool key Down Down Return; sleep 3
xdotool mousemove 550 307 click 1; sleep 0.5; xdotool key ctrl+alt+k; sleep 0.3; xdotool key Tab
xdotool mousemove 450 493 click 1; sleep 0.3
xdotool mousemove 800 538 click 1; sleep 2
SETTINGS_FILE=$(find "$WINEPREFIX/drive_c/users" -path "*RustySnapr/settings.json" | head -1)
check "settings are saved" grep -q '"Hotkey": "Ctrl+Alt+K"' "$SETTINGS_FILE"
check "start with Windows is saved" grep -q '"StartWithWindows": true' "$SETTINGS_FILE"
check "start with Windows is registered" sh -c "wine reg query 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run' /v RustySnapr 2>/dev/null | grep -q Snapr.exe"

wineserver -k
[ -n "${KEEP:-}" ] && echo "kept $WORK" || rm -rf "$WORK"
[ $failures -eq 0 ] && echo "all passed" || { echo "$failures failed"; exit 1; }
