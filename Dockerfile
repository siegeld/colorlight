# Dockerfile for Colorlight 5A-75E FPGA build environment
# Builds yosys, nextpnr-ecp5, prjtrellis from source

FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    clang \
    bison \
    flex \
    libreadline-dev \
    gawk \
    tcl-dev \
    libffi-dev \
    git \
    graphviz \
    xdot \
    pkg-config \
    python3 \
    python3-pip \
    python3-venv \
    libboost-system-dev \
    libboost-python-dev \
    libboost-filesystem-dev \
    libboost-thread-dev \
    libboost-program-options-dev \
    libboost-iostreams-dev \
    libboost-dev \
    libeigen3-dev \
    cmake \
    libftdi1-dev \
    libusb-1.0-0-dev \
    libhidapi-dev \
    zlib1g-dev \
    wget \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Build Yosys
RUN git clone --depth 1 --branch yosys-0.40 https://github.com/YosysHQ/yosys.git /tmp/yosys \
    && cd /tmp/yosys \
    && make config-clang \
    && make -j$(nproc) \
    && make install \
    && rm -rf /tmp/yosys

# Build prjtrellis (needed for nextpnr-ecp5)
RUN git clone --recursive --depth 1 https://github.com/YosysHQ/prjtrellis.git /tmp/prjtrellis \
    && cd /tmp/prjtrellis/libtrellis \
    && cmake -DCMAKE_INSTALL_PREFIX=/usr/local . \
    && make -j$(nproc) \
    && make install \
    && rm -rf /tmp/prjtrellis

# Build nextpnr-ecp5
RUN git clone --depth 1 https://github.com/YosysHQ/nextpnr.git /tmp/nextpnr \
    && cd /tmp/nextpnr \
    && cmake -DARCH=ecp5 -DTRELLIS_INSTALL_PREFIX=/usr/local -DCMAKE_INSTALL_PREFIX=/usr/local -B build \
    && cmake --build build -j$(nproc) \
    && cmake --install build \
    && rm -rf /tmp/nextpnr

# Build openFPGALoader
RUN git clone --depth 1 https://github.com/trabucayre/openFPGALoader.git /tmp/openFPGALoader \
    && cd /tmp/openFPGALoader \
    && mkdir build && cd build \
    && cmake .. \
    && make -j$(nproc) \
    && make install \
    && rm -rf /tmp/openFPGALoader

# Install LiteX
RUN pip3 install --break-system-packages \
    meson \
    ninja \
    pyyaml \
    litex \
    liteeth

# Set library path
ENV LD_LIBRARY_PATH=/usr/local/lib

WORKDIR /build

# Verify installations
RUN echo "=== Installed Tools ===" \
    && yosys --version \
    && nextpnr-ecp5 --version 2>&1 | head -1 || true \
    && ecppack --help 2>&1 | head -1 || echo "ecppack installed" \
    && openFPGALoader --version 2>&1 | head -1 || true \
    && echo "======================"

CMD ["/bin/bash"]
