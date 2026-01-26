# LiteX build environment for hub75_colorlight75_stuff
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive

# Install base dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    git \
    wget \
    curl \
    python3 \
    python3-pip \
    python3-venv \
    libevent-dev \
    libjson-c-dev \
    autoconf \
    flex \
    bison \
    libftdi1-dev \
    libusb-1.0-0-dev \
    pkg-config \
    cmake \
    libboost-all-dev \
    libeigen3-dev \
    xz-utils \
    && rm -rf /var/lib/apt/lists/*

# Install OSS CAD Suite (includes yosys, nextpnr-ecp5, prjtrellis, ecpprog)
RUN cd /opt && \
    wget -q https://github.com/YosysHQ/oss-cad-suite-build/releases/download/2024-02-14/oss-cad-suite-linux-x64-20240214.tgz && \
    tar -xzf oss-cad-suite-linux-x64-20240214.tgz && \
    rm oss-cad-suite-linux-x64-20240214.tgz

ENV PATH="/opt/oss-cad-suite/bin:${PATH}"

# Install RISC-V toolchain from xPack (has proper prefixes)
RUN mkdir -p /opt/xpack-riscv && \
    cd /tmp && \
    wget -q https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases/download/v13.2.0-2/xpack-riscv-none-elf-gcc-13.2.0-2-linux-x64.tar.gz && \
    tar -xzf xpack-riscv-none-elf-gcc-13.2.0-2-linux-x64.tar.gz -C /opt/xpack-riscv --strip-components=1 && \
    rm xpack-riscv-none-elf-gcc-13.2.0-2-linux-x64.tar.gz

ENV PATH="/opt/xpack-riscv/bin:${PATH}"

# Install Python dependencies
RUN pip3 install --no-cache-dir pypng meson ninja

# Install LiteX
WORKDIR /litex
RUN wget -q https://raw.githubusercontent.com/enjoy-digital/litex/2025.12/litex_setup.py && \
    chmod +x litex_setup.py && \
    ./litex_setup.py --init --install --tag=2025.12

# Apply local patches to LiteX BIOS
COPY patches/ /litex/patches/
RUN cd /litex/litex && \
    patch -p1 < /litex/patches/litex-bios-broadcast.patch

# Install Rust for firmware
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
    . /root/.cargo/env && \
    rustup target add riscv32i-unknown-none-elf && \
    cargo install svd2rust form

ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /project

# No ENTRYPOINT - use bash -c in build script for flexibility
CMD ["/bin/bash"]
