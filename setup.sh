#!/bin/bash
#
# Setup script for Colorlight LED display project
#
# This script:
# - Checks for required host tools (Docker, openFPGALoader)
# - Creates Python virtual environment
# - Installs Python dependencies
# - Sets up udev rules for USB Blaster
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "========================================"
echo "Colorlight 5A-75E Setup"
echo "========================================"
echo ""

# Track what needs to be done
NEED_DOCKER=false
NEED_FPGALOADER=false
NEED_UDEV=false

# Check Docker
echo "Checking Docker..."
if command -v docker &> /dev/null; then
    if docker info &> /dev/null; then
        echo "  [OK] Docker is installed and running"
    else
        echo "  [!!] Docker is installed but not running or no permission"
        echo "       Try: sudo systemctl start docker"
        echo "       Or:  sudo usermod -aG docker $USER (then logout/login)"
    fi
else
    echo "  [!!] Docker not found"
    NEED_DOCKER=true
fi

# Check openFPGALoader
echo "Checking openFPGALoader..."
if command -v openFPGALoader &> /dev/null; then
    echo "  [OK] openFPGALoader is installed"
else
    echo "  [!!] openFPGALoader not found (needed for flashing)"
    NEED_FPGALOADER=true
fi

# Check udev rules for USB Blaster
echo "Checking udev rules..."
if [ -f /etc/udev/rules.d/51-altera-usb-blaster.rules ]; then
    echo "  [OK] USB Blaster udev rules exist"
else
    echo "  [!!] USB Blaster udev rules not found"
    NEED_UDEV=true
fi

# Check Python
echo "Checking Python..."
if command -v python3 &> /dev/null; then
    PYTHON_VERSION=$(python3 --version 2>&1)
    echo "  [OK] $PYTHON_VERSION"
else
    echo "  [!!] Python 3 not found"
fi

echo ""

# Show what needs to be installed
if [ "$NEED_DOCKER" = true ] || [ "$NEED_FPGALOADER" = true ]; then
    echo "========================================"
    echo "Missing Dependencies"
    echo "========================================"
    echo ""

    if [ "$NEED_DOCKER" = true ]; then
        echo "Install Docker:"
        echo "  Fedora: sudo dnf install docker docker-compose"
        echo "          sudo systemctl enable --now docker"
        echo "          sudo usermod -aG docker $USER"
        echo ""
    fi

    if [ "$NEED_FPGALOADER" = true ]; then
        echo "Install openFPGALoader:"
        echo "  Fedora: sudo dnf install openFPGALoader"
        echo "  Ubuntu: sudo apt install openfpgaloader"
        echo "  Arch:   sudo pacman -S openfpgaloader"
        echo ""
    fi
fi

# Setup udev rules
if [ "$NEED_UDEV" = true ]; then
    echo "========================================"
    echo "Setting up USB Blaster udev rules..."
    echo "========================================"

    sudo tee /etc/udev/rules.d/51-altera-usb-blaster.rules > /dev/null << 'EOF'
# Altera USB Blaster
SUBSYSTEM=="usb", ATTR{idVendor}=="09fb", ATTR{idProduct}=="6001", MODE="0666"
SUBSYSTEM=="usb", ATTR{idVendor}=="09fb", ATTR{idProduct}=="6010", MODE="0666"
SUBSYSTEM=="usb", ATTR{idVendor}=="09fb", ATTR{idProduct}=="6810", MODE="0666"
EOF

    sudo udevadm control --reload-rules
    sudo udevadm trigger
    echo "  [OK] udev rules installed"
    echo ""
fi

# Setup Python venv
echo "========================================"
echo "Setting up Python environment..."
echo "========================================"

if [ ! -d "$SCRIPT_DIR/venv" ]; then
    echo "Creating virtual environment..."
    python3 -m venv "$SCRIPT_DIR/venv"
fi

echo "Installing Python dependencies..."
source "$SCRIPT_DIR/venv/bin/activate"
pip install --quiet --upgrade pip
pip install --quiet -r "$SCRIPT_DIR/requirements.txt"
echo "  [OK] Python environment ready"
echo ""

# Build Docker image
echo "========================================"
echo "Building Docker image..."
echo "========================================"
echo "(This may take 10-15 minutes on first run)"
echo ""

if docker info &> /dev/null; then
    "$SCRIPT_DIR/docker_build.sh" build-image
else
    echo "  [SKIP] Docker not available, skipping image build"
fi

echo ""
echo "========================================"
echo "Setup Complete!"
echo "========================================"
echo ""
echo "Next steps:"
echo ""
echo "1. Build firmware:"
echo "   ./docker_build.sh blink      # LED blink test"
echo "   ./docker_build.sh hub75      # HUB75 panel test"
echo "   ./docker_build.sh firmware   # Full network firmware"
echo ""
echo "2. Flash to board:"
echo "   ./flash.sh blink             # Flash blink test"
echo "   ./flash.sh hub75             # Flash HUB75 test"
echo "   ./flash.sh                   # Flash network firmware"
echo ""
echo "3. Run Python examples:"
echo "   source venv/bin/activate"
echo "   python python/examples/test_connection.py"
echo ""
