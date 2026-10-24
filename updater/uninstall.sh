#!/bin/bash
# Entfernt den ArtCraft-Updater wieder (die installierten Apps bleiben).
LABEL="com.76344243.craft-updater"
/bin/launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null
rm -f "$HOME/Library/LaunchAgents/$LABEL.plist"
rm -rf "$HOME/Library/Application Support/ArtCraft-Updater"
echo "ArtCraft-Updater entfernt."
