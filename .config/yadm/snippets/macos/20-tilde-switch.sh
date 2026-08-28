#!/bin/bash

print_bold -ad "Applying tilde-switch"

TILDE_PLIST="/Library/LaunchDaemons/org.custom.tilde-switch.plist"
TILDE_HIDUTIL='{"UserKeyMapping":[
    {"HIDKeyboardModifierMappingSrc":0x700000035,"HIDKeyboardModifierMappingDst":0x700000035},
    {"HIDKeyboardModifierMappingSrc":0x700000064,"HIDKeyboardModifierMappingDst":0x700000035}
]}'

# Create ~/.tilde-switch script if it does not already exist
if [ ! -f "$HOME/.tilde-switch" ]; then
    cat <<EOF >"$HOME/.tilde-switch"
#!/bin/bash
sudo hidutil property --set '$TILDE_HIDUTIL'
EOF
    chmod +x "$HOME/.tilde-switch"
    print_success "Created $HOME/.tilde-switch"
else
    print_info "$HOME/.tilde-switch already exists. Skipping."
fi

# The daemon is the machine-global half, and it is RunAtLoad — so once the plist
# is in place it loads on every boot and there is nothing left to do. Test for
# the plist and stop there.
#
# The previous version asked `sudo launchctl list` instead, so a fully
# configured machine paid three password prompts for a no-op; and it printed
# "Launch daemon loaded" and "Tilde-switch logic executed" whether or not the
# sudo commands behind them had succeeded. From an account without sudo rights
# it claimed success three times having done nothing at all.
if [ -f "$TILDE_PLIST" ]; then
    print_info "$TILDE_PLIST already exists. Skipping."
    return
fi

print_bold "Creating $TILDE_PLIST..."
TILDE_TMP="$(mktemp -t tilde-switch)"
cat <<EOF >"$TILDE_TMP"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>org.custom.tilde-switch</string>
    <key>Program</key>
    <string>${HOME}/.tilde-switch</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
  </dict>
</plist>
EOF

if sudo install -m 644 -o root -g wheel "$TILDE_TMP" "$TILDE_PLIST"; then
    rm -f "$TILDE_TMP"
    print_success "Created $TILDE_PLIST"
else
    rm -f "$TILDE_TMP"
    print_error -ad "Could not write $TILDE_PLIST (needs admin rights); tilde-switch not configured"
    return
fi

print_bold "Loading the launch daemon org.custom.tilde-switch..."
if sudo launchctl load -w -- "$TILDE_PLIST"; then
    print_success "Launch daemon loaded"
else
    print_error -ad "Could not load the launch daemon; the mapping applies at next boot"
fi

print_bold "Applying the key mapping to the current session..."
if sudo hidutil property --set "$TILDE_HIDUTIL" >/dev/null; then
    print_success "Key mapping applied"
else
    print_error -ad "Could not apply the key mapping now; it applies at next boot"
fi
