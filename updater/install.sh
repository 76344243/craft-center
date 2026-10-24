#!/bin/bash
# Richtet den ArtCraft-Updater einmalig ein (ohne Administratorrechte).
# Aufruf im Terminal:  bash ~/Dev/ArtCraft-Fork/updater/install.sh
set -eu
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIR="$HOME/Library/Application Support/ArtCraft-Updater"
LABEL="com.76344243.craft-updater"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"

mkdir -p "$DIR" "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
cp "$HERE/craft-update.sh" "$DIR/craft-update.sh"
chmod 755 "$DIR/craft-update.sh"
/usr/bin/xattr -c "$DIR/craft-update.sh" 2>/dev/null || true

cat >"$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$LABEL</string>
  <key>ProgramArguments</key>
  <array>
    <string>/bin/bash</string>
    <string>$DIR/craft-update.sh</string>
  </array>
  <key>RunAtLoad</key><true/>
  <key>StartInterval</key><integer>7200</integer>
  <key>ProcessType</key><string>Background</string>
  <key>LowPriorityIO</key><true/>
</dict>
</plist>
EOF

/bin/launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
/bin/launchctl bootstrap "gui/$(id -u)" "$PLIST"

echo "Fertig. Der Updater prüft jetzt und danach alle 2 Stunden auf neue Versionen."
echo "Laufende Apps werden nicht angefasst; bitte WordCraft und PdfCraft jetzt einmal beenden."
echo "Protokoll: ~/Library/Logs/ArtCraft-Updater.log"
