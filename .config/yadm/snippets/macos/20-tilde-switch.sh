#!/bin/bash

print_bold -ad "Applying tilde-switch"

# Create ~/.tilde-switch script if it does not already exist
if [ ! -f "$HOME/.tilde-switch" ]; then
    echo "Creating $HOME/.tilde-switch script..."
    cat <<'EOF' >"$HOME/.tilde-switch"
#!/bin/bash
sudo hidutil property --set '{"UserKeyMapping":[
    {"HIDKeyboardModifierMappingSrc":0x700000035,"HIDKeyboardModifierMappingDst":0x700000035},
    {"HIDKeyboardModifierMappingSrc":0x700000064,"HIDKeyboardModifierMappingDst":0x700000035}
]}'
EOF
    chmod +x "$HOME/.tilde-switch"
    echo "$HOME/.tilde-switch script created and made executable."
else
    echo "$HOME/.tilde-switch script already exists. Skipping creation."
fi

# Create /Library/LaunchDaemons/org.custom.tilde-switch.plist if it does not already exist
if [ ! -f "/Library/LaunchDaemons/org.custom.tilde-switch.plist" ]; then
    echo "Creating /Library/LaunchDaemons/org.custom.tilde-switch.plist..."
    sudo /usr/bin/env bash -c "cat > /Library/LaunchDaemons/org.custom.tilde-switch.plist" <<EOF
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
    echo "/Library/LaunchDaemons/org.custom.tilde-switch.plist created."
else
    echo "/Library/LaunchDaemons/org.custom.tilde-switch.plist already exists. Skipping creation."
fi

# Load the launch daemon if it is not already loaded
if ! sudo launchctl list | grep -q "org.custom.tilde-switch"; then
    echo "Loading the launch daemon org.custom.tilde-switch..."
    sudo launchctl load -w -- /Library/LaunchDaemons/org.custom.tilde-switch.plist
    echo "Launch daemon org.custom.tilde-switch loaded."

    # Execute tilde-switch now, so that it is in effect immediately
    echo "Executing tilde-switch logic for current session..."
    sudo hidutil property --set '{"UserKeyMapping":[
        {"HIDKeyboardModifierMappingSrc":0x700000035,"HIDKeyboardModifierMappingDst":0x700000035},
        {"HIDKeyboardModifierMappingSrc":0x700000064,"HIDKeyboardModifierMappingDst":0x700000035}
    ]}'
    echo "Tilde-switch logic executed."
else
    echo "Launch daemon org.custom.tilde-switch is already loaded. Skipping load."
fi
