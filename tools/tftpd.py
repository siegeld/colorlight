#!/usr/bin/env python3
"""Minimal TFTP server on a custom port (default 6969).

Uses tftpy to serve files from a root directory. Designed to run as a
background daemon started by build.sh.

Usage:
    python3 tools/tftpd.py --root .tftp --host 10.11.6.65 --port 6969
"""

import argparse
import logging
import os
import signal
import sys

import tftpy


def main():
    parser = argparse.ArgumentParser(description="TFTP server on custom port")
    parser.add_argument("--root", required=True, help="TFTP root directory")
    parser.add_argument("--host", default="0.0.0.0", help="Listen address")
    parser.add_argument("--port", type=int, default=6969, help="Listen port")
    parser.add_argument("--log", help="Log file path")
    parser.add_argument("--pid", help="PID file path")
    args = parser.parse_args()

    # Set up logging
    handlers = [logging.StreamHandler()]
    if args.log:
        handlers.append(logging.FileHandler(args.log))
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(message)s",
        datefmt="%b %d %H:%M:%S",
        handlers=handlers,
    )
    # tftpy uses the 'tftpy' logger
    logging.getLogger("tftpy").setLevel(logging.INFO)

    # Write PID file
    if args.pid:
        with open(args.pid, "w") as f:
            f.write(str(os.getpid()))

    def cleanup(signum, frame):
        if args.pid and os.path.exists(args.pid):
            os.remove(args.pid)
        sys.exit(0)

    signal.signal(signal.SIGTERM, cleanup)
    signal.signal(signal.SIGINT, cleanup)

    server = tftpy.TftpServer(args.root)
    logging.info("TFTP server listening on %s:%d, root=%s", args.host, args.port, args.root)
    try:
        server.listen(args.host, args.port)
    except KeyboardInterrupt:
        pass
    finally:
        cleanup(None, None)


if __name__ == "__main__":
    main()
