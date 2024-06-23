#!/bin/bash

# Detect the Linux distribution
if [ -f /etc/debian_version ]; then
    DISTRO="debian"
elif [ -f /etc/redhat-release ]; then
    DISTRO="redhat"
elif [ -f /etc/arch-release ]; then
    DISTRO="arch"
elif [ -f /etc/SuSE-release ]; then
    DISTRO="suse"
else
    echo "Unsupported Linux distribution"
    exit 1
fi

# Update package lists and install Git based on the distribution
case "$DISTRO" in
debian)
    echo "Detected Debian-based distribution"
    sudo apt-get update
    if ! command -v git &>/dev/null; then
        echo "Installing Git..."
        sudo apt-get install -y git
    else
        echo "Git is already installed"
    fi
    ;;
redhat)
    echo "Detected Red Hat-based distribution"
    sudo yum check-update
    if ! command -v git &>/dev/null; then
        echo "Installing Git..."
        sudo yum install -y git
    else
        echo "Git is already installed"
    fi
    ;;
arch)
    echo "Detected Arch-based distribution"
    sudo pacman -Sy
    if ! command -v git &>/dev/null; then
        echo "Installing Git..."
        sudo pacman -S --noconfirm git
    else
        echo "Git is already installed"
    fi
    ;;
suse)
    echo "Detected SUSE-based distribution"
    sudo zypper refresh
    if ! command -v git &>/dev/null; then
        echo "Installing Git..."
        sudo zypper install -y git
    else
        echo "Git is already installed"
    fi
    ;;
*)
    echo "Unsupported Linux distribution: $DISTRO"
    exit 1
    ;;
esac
