#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IMAGE_NAME="litex-hub75"

# Build Docker image if needed
if ! docker image inspect "$IMAGE_NAME" &>/dev/null; then
    echo "Building Docker image (this may take a while)..."
    docker build -t "$IMAGE_NAME" "$SCRIPT_DIR"
fi

# Default to revision 8.0 and build
REVISION="${1:-8.0}"
ACTION="${2:---build}"

echo "Running: ./colorlight.py --revision $REVISION $ACTION"

docker run --rm -it \
    -v "$SCRIPT_DIR:/project" \
    -v /dev/bus/usb:/dev/bus/usb \
    --privileged \
    "$IMAGE_NAME" \
    "./colorlight.py --revision $REVISION $ACTION"
